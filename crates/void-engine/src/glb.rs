//! Small static GLB reader. Honours scene nodes, accessor offsets and byte strides.
use crate::mesh::{MeshData, Vertex};
use anyhow::{bail, Context, Result};
use glam::{Mat4, Quat, Vec3};
use serde_json::Value;
use std::path::Path;

pub struct Model {
    pub mesh: MeshData,
    pub image: Option<image::RgbaImage>,
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        data.get(offset..offset + 4)
            .context("Truncated GLB")?
            .try_into()?,
    ))
}

pub fn load(path: &Path) -> Result<Model> {
    let bytes = std::fs::read(path).with_context(|| format!("Reading {}", path.display()))?;
    if u32_at(&bytes, 0)? != 0x46546c67 || u32_at(&bytes, 4)? != 2 {
        bail!("Expected GLB 2.0");
    }
    let length = u32_at(&bytes, 8)? as usize;
    if length != bytes.len() {
        bail!("Invalid GLB length");
    }
    let mut json = None;
    let mut bin = &[][..];
    let mut offset = 12;
    while offset + 8 <= length {
        let size = u32_at(&bytes, offset)? as usize;
        let kind = u32_at(&bytes, offset + 4)?;
        let chunk = bytes
            .get(offset + 8..offset + 8 + size)
            .context("Invalid GLB chunk")?;
        if kind == 0x4e4f534a {
            json = Some(serde_json::from_slice::<Value>(chunk)?);
        }
        if kind == 0x004e4942 {
            bin = chunk;
        }
        offset += 8 + size;
    }
    let doc = json.context("Missing GLB JSON")?;
    let mut model = Model {
        mesh: MeshData::default(),
        image: None,
    };
    let scene = doc["scene"].as_u64().unwrap_or(0) as usize;
    if let Some(nodes) = doc["scenes"][scene]["nodes"].as_array() {
        for node in nodes {
            visit(
                &doc,
                bin,
                node.as_u64().context("Invalid node")? as usize,
                Mat4::IDENTITY,
                &mut model.mesh,
                0,
            )?;
        }
    } else {
        for i in 0..doc["meshes"].as_array().context("Missing meshes")?.len() {
            read_mesh(&doc, bin, i, Mat4::IDENTITY, &mut model.mesh)?;
        }
    }
    if let Some(view) = doc["images"][0]["bufferView"].as_u64() {
        let v = &doc["bufferViews"][view as usize];
        let start = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        let len = v["byteLength"].as_u64().context("Invalid image view")? as usize;
        model.image = Some(
            image::load_from_memory(bin.get(start..start + len).context("Image outside BIN")?)?
                .to_rgba8(),
        );
    }
    if model.mesh.vertices.is_empty() {
        bail!("GLB contains no static triangles");
    }
    Ok(model)
}

fn array<const N: usize>(value: &Value, fallback: [f32; N]) -> [f32; N] {
    std::array::from_fn(|i| value[i].as_f64().map(|v| v as f32).unwrap_or(fallback[i]))
}

fn visit(
    doc: &Value,
    bin: &[u8],
    id: usize,
    parent: Mat4,
    out: &mut MeshData,
    depth: u32,
) -> Result<()> {
    if depth > 64 {
        bail!("GLB node hierarchy too deep");
    }
    let node = &doc["nodes"][id];
    let local = if node["matrix"].is_array() {
        Mat4::from_cols_array(&array(&node["matrix"], Mat4::IDENTITY.to_cols_array()))
    } else {
        Mat4::from_scale_rotation_translation(
            Vec3::from(array(&node["scale"], [1.0; 3])),
            Quat::from_array(array(&node["rotation"], [0.0, 0.0, 0.0, 1.0])),
            Vec3::from(array(&node["translation"], [0.0; 3])),
        )
    };
    let world = parent * local;
    if let Some(id) = node["mesh"].as_u64() {
        read_mesh(doc, bin, id as usize, world, out)?;
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            visit(
                doc,
                bin,
                child.as_u64().context("Invalid child")? as usize,
                world,
                out,
                depth + 1,
            )?;
        }
    }
    Ok(())
}

fn accessor(doc: &Value, bin: &[u8], id: usize, components: usize) -> Result<Vec<Vec<f32>>> {
    let a = &doc["accessors"][id];
    let v = &doc["bufferViews"][a["bufferView"]
        .as_u64()
        .context("Sparse accessors unsupported")? as usize];
    let kind = a["componentType"]
        .as_u64()
        .context("Missing component type")?;
    let size = match kind {
        5126 | 5125 => 4,
        5123 => 2,
        5121 => 1,
        _ => bail!("Unsupported accessor type {kind}"),
    };
    let stride = v["byteStride"]
        .as_u64()
        .unwrap_or((size * components) as u64) as usize;
    let start =
        (v["byteOffset"].as_u64().unwrap_or(0) + a["byteOffset"].as_u64().unwrap_or(0)) as usize;
    let count = a["count"].as_u64().context("Missing accessor count")? as usize;
    let mut result = Vec::with_capacity(count);
    for i in 0..count {
        let mut values = Vec::with_capacity(components);
        for c in 0..components {
            let pos = start + i * stride + c * size;
            let data = bin.get(pos..pos + size).context("Accessor outside BIN")?;
            values.push(match kind {
                5126 => f32::from_le_bytes(data.try_into()?),
                5125 => u32::from_le_bytes(data.try_into()?) as f32,
                5123 => u16::from_le_bytes(data.try_into()?) as f32,
                _ => data[0] as f32,
            });
        }
        result.push(values);
    }
    Ok(result)
}

fn read_mesh(
    doc: &Value,
    bin: &[u8],
    id: usize,
    transform: Mat4,
    out: &mut MeshData,
) -> Result<()> {
    for primitive in doc["meshes"][id]["primitives"]
        .as_array()
        .context("Invalid mesh")?
    {
        if primitive["mode"].as_u64().unwrap_or(4) != 4 {
            continue;
        }
        let a = &primitive["attributes"];
        let positions = accessor(
            doc,
            bin,
            a["POSITION"].as_u64().context("Missing positions")? as usize,
            3,
        )?;
        let normals = a["NORMAL"]
            .as_u64()
            .map(|id| accessor(doc, bin, id as usize, 3))
            .transpose()?;
        let uvs = a["TEXCOORD_0"]
            .as_u64()
            .map(|id| accessor(doc, bin, id as usize, 2))
            .transpose()?;
        let mut mesh = MeshData::default();
        for (i, p) in positions.iter().enumerate() {
            mesh.vertices.push(Vertex {
                position: [p[0], p[1], p[2]],
                normal: normals
                    .as_ref()
                    .map(|v| [v[i][0], v[i][1], v[i][2]])
                    .unwrap_or([0.0, 1.0, 0.0]),
                uv: uvs.as_ref().map(|v| [v[i][0], v[i][1]]).unwrap_or([0.0; 2]),
            });
        }
        mesh.indices = if let Some(id) = primitive["indices"].as_u64() {
            accessor(doc, bin, id as usize, 1)?
                .iter()
                .map(|v| v[0] as u32)
                .collect()
        } else {
            (0..positions.len() as u32).collect()
        };
        if mesh
            .indices
            .iter()
            .any(|&i| i as usize >= mesh.vertices.len())
        {
            bail!("GLB index out of bounds");
        }
        out.append(&mesh, transform);
    }
    Ok(())
}
