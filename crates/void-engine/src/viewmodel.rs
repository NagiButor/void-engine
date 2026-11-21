//! Rigged glTF 2.0 viewmodel loader, skeletal animation blender, and CPU vertex skinner.
use crate::mesh::{MeshData, Vertex};
use anyhow::{bail, Context, Result};
use glam::{Mat3, Mat4, Quat, Vec3};
use serde_json::Value;
use std::{collections::HashMap, path::Path};

#[derive(Clone)]
pub struct SkinnedSubmesh {
    pub name: String,
    pub material: usize,
    pub rest: MeshData,
    pub joints: Vec<[u8; 4]>,
    pub weights: Vec<[f32; 4]>,
}

#[derive(Clone)]
pub struct NodeRest {
    pub name: String,
    pub parent: Option<usize>,
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

#[derive(Clone)]
pub struct AnimChannel {
    pub node: usize,
    pub times: Vec<f32>,
    pub translations: Option<Vec<Vec3>>,
    pub rotations: Option<Vec<Quat>>,
    pub scales: Option<Vec<Vec3>>,
}

#[derive(Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<AnimChannel>,
}

pub struct RiggedModel {
    pub nodes: Vec<NodeRest>,
    pub order: Vec<usize>,
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<Mat4>,
    pub submeshes: Vec<SkinnedSubmesh>,
    pub animations: Vec<AnimationClip>,
    pub anim_index: HashMap<String, usize>,
    pub rif_node: Option<usize>,
    pub pbody_node: Option<usize>,
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        data.get(offset..offset + 4)
            .context("Truncated GLB")?
            .try_into()?,
    ))
}

fn read_bytes<'a>(doc: &Value, bin: &'a [u8], acc_id: usize) -> Result<(&'a [u8], usize, u64)> {
    let a = &doc["accessors"][acc_id];
    let bv = &doc["bufferViews"][a["bufferView"].as_u64().context("Missing bufferView")? as usize];
    let comp_type = a["componentType"]
        .as_u64()
        .context("Missing componentType")?;
    let count = a["count"].as_u64().context("Missing count")? as usize;
    let start =
        (bv["byteOffset"].as_u64().unwrap_or(0) + a["byteOffset"].as_u64().unwrap_or(0)) as usize;
    let len = bv["byteLength"].as_u64().context("Missing byteLength")? as usize;
    let slice = bin.get(start..start + len).context("Accessor out of BIN")?;
    Ok((slice, count, comp_type))
}

fn read_f32_vec(doc: &Value, bin: &[u8], acc_id: usize, comps: usize) -> Result<Vec<f32>> {
    let (bytes, count, comp_type) = read_bytes(doc, bin, acc_id)?;
    if comp_type != 5126 {
        bail!("Expected f32 accessor, got {comp_type}");
    }
    let need = count * comps * 4;
    if bytes.len() < need {
        bail!("Truncated f32 accessor");
    }
    let mut out = Vec::with_capacity(count * comps);
    for &chunk in bytes[..need].as_chunks::<4>().0 {
        out.push(f32::from_le_bytes(chunk));
    }
    Ok(out)
}

fn read_u32_indices(doc: &Value, bin: &[u8], acc_id: usize) -> Result<Vec<u32>> {
    let (bytes, count, comp_type) = read_bytes(doc, bin, acc_id)?;
    let mut out = Vec::with_capacity(count);
    match comp_type {
        5125 => {
            for &c in bytes[..count * 4].as_chunks::<4>().0 {
                out.push(u32::from_le_bytes(c));
            }
        }
        5123 => {
            for &c in bytes[..count * 2].as_chunks::<2>().0 {
                out.push(u16::from_le_bytes(c) as u32);
            }
        }
        5121 => {
            out.extend(bytes[..count].iter().map(|&b| b as u32));
        }
        _ => bail!("Unsupported index type {comp_type}"),
    }
    Ok(out)
}

fn read_joints(doc: &Value, bin: &[u8], acc_id: usize) -> Result<Vec<[u8; 4]>> {
    let (bytes, count, comp_type) = read_bytes(doc, bin, acc_id)?;
    let mut out = Vec::with_capacity(count);
    match comp_type {
        5121 => {
            for &c in bytes[..count * 4].as_chunks::<4>().0 {
                out.push(c);
            }
        }
        5123 => {
            for &c in bytes[..count * 8].as_chunks::<8>().0 {
                out.push([
                    u16::from_le_bytes([c[0], c[1]]) as u8,
                    u16::from_le_bytes([c[2], c[3]]) as u8,
                    u16::from_le_bytes([c[4], c[5]]) as u8,
                    u16::from_le_bytes([c[6], c[7]]) as u8,
                ]);
            }
        }
        _ => bail!("Unsupported JOINTS_0 type {comp_type}"),
    }
    Ok(out)
}

fn json_vec3(v: &Value, fallback: [f32; 3]) -> Vec3 {
    Vec3::new(
        v[0].as_f64().map(|x| x as f32).unwrap_or(fallback[0]),
        v[1].as_f64().map(|x| x as f32).unwrap_or(fallback[1]),
        v[2].as_f64().map(|x| x as f32).unwrap_or(fallback[2]),
    )
}

fn json_quat(v: &Value) -> Quat {
    Quat::from_xyzw(
        v[0].as_f64().map(|x| x as f32).unwrap_or(0.0),
        v[1].as_f64().map(|x| x as f32).unwrap_or(0.0),
        v[2].as_f64().map(|x| x as f32).unwrap_or(0.0),
        v[3].as_f64().map(|x| x as f32).unwrap_or(1.0),
    )
    .normalize()
}

fn sample_vec3(times: &[f32], vals: &[Vec3], t: f32) -> Vec3 {
    if vals.is_empty() {
        return Vec3::ZERO;
    }
    if t <= times[0] || vals.len() == 1 {
        return vals[0];
    }
    let last = times.len() - 1;
    if t >= times[last] {
        return vals[last];
    }
    let idx = times.partition_point(|&kt| kt <= t).saturating_sub(1);
    let i1 = (idx + 1).min(last);
    let span = (times[i1] - times[idx]).max(1e-6);
    let a = ((t - times[idx]) / span).clamp(0.0, 1.0);
    vals[idx].lerp(vals[i1], a)
}

fn sample_quat(times: &[f32], vals: &[Quat], t: f32) -> Quat {
    if vals.is_empty() {
        return Quat::IDENTITY;
    }
    if t <= times[0] || vals.len() == 1 {
        return vals[0];
    }
    let last = times.len() - 1;
    if t >= times[last] {
        return vals[last];
    }
    let idx = times.partition_point(|&kt| kt <= t).saturating_sub(1);
    let i1 = (idx + 1).min(last);
    let span = (times[i1] - times[idx]).max(1e-6);
    let a = ((t - times[idx]) / span).clamp(0.0, 1.0);
    vals[idx].slerp(vals[i1], a)
}

impl RiggedModel {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("Reading {}", path.display()))?;
        if u32_at(&bytes, 0)? != 0x46546c67 || u32_at(&bytes, 4)? != 2 {
            bail!("Expected GLB 2.0");
        }
        let length = u32_at(&bytes, 8)? as usize;
        if length != bytes.len() {
            bail!("Invalid GLB length");
        }
        let mut doc = None;
        let mut bin = &[][..];
        let mut offset = 12;
        while offset + 8 <= length {
            let size = u32_at(&bytes, offset)? as usize;
            let kind = u32_at(&bytes, offset + 4)?;
            let chunk = bytes
                .get(offset + 8..offset + 8 + size)
                .context("Invalid GLB chunk")?;
            if kind == 0x4e4f534a {
                doc = Some(serde_json::from_slice::<Value>(chunk)?);
            } else if kind == 0x004e4942 {
                bin = chunk;
            }
            offset += 8 + size;
        }
        let doc = doc.context("Missing GLB JSON")?;

        let raw_nodes = doc["nodes"].as_array().context("Missing nodes")?;
        let mut nodes: Vec<NodeRest> = raw_nodes
            .iter()
            .map(|n| NodeRest {
                name: n["name"].as_str().unwrap_or("").to_owned(),
                parent: None,
                translation: json_vec3(&n["translation"], [0.0; 3]),
                rotation: json_quat(&n["rotation"]),
                scale: json_vec3(&n["scale"], [1.0; 3]),
            })
            .collect();
        for (p_idx, n) in raw_nodes.iter().enumerate() {
            if let Some(children) = n["children"].as_array() {
                for ch in children {
                    if let Some(c_idx) = ch.as_u64().map(|v| v as usize) {
                        if c_idx < nodes.len() {
                            nodes[c_idx].parent = Some(p_idx);
                        }
                    }
                }
            }
        }

        // Topological parent-before-child evaluation order.
        let mut order = Vec::with_capacity(nodes.len());
        let mut visited = vec![false; nodes.len()];
        fn visit_topo(
            idx: usize,
            raw_nodes: &[Value],
            visited: &mut [bool],
            order: &mut Vec<usize>,
        ) {
            if visited[idx] {
                return;
            }
            visited[idx] = true;
            order.push(idx);
            if let Some(children) = raw_nodes[idx]["children"].as_array() {
                for ch in children {
                    if let Some(ci) = ch.as_u64().map(|v| v as usize) {
                        visit_topo(ci, raw_nodes, visited, order);
                    }
                }
            }
        }
        for (i, node) in nodes.iter().enumerate() {
            if node.parent.is_none() {
                visit_topo(i, raw_nodes, &mut visited, &mut order);
            }
        }

        let skin = &doc["skins"][0];
        let joints: Vec<usize> = skin["joints"]
            .as_array()
            .context("Missing skin joints")?
            .iter()
            .filter_map(|v| v.as_u64().map(|x| x as usize))
            .collect();
        let ibm_floats = read_f32_vec(
            &doc,
            bin,
            skin["inverseBindMatrices"]
                .as_u64()
                .context("Missing inverseBindMatrices")? as usize,
            16,
        )?;
        let inverse_bind: Vec<Mat4> = ibm_floats
            .as_chunks::<16>()
            .0
            .iter()
            .map(Mat4::from_cols_array)
            .collect();

        let mut submeshes = Vec::new();
        for m in doc["meshes"].as_array().context("Missing meshes")? {
            let name = m["name"].as_str().unwrap_or("mesh").to_owned();
            let prim = &m["primitives"][0];
            let attr = &prim["attributes"];
            let pos = read_f32_vec(&doc, bin, attr["POSITION"].as_u64().unwrap() as usize, 3)?;
            let norm = read_f32_vec(&doc, bin, attr["NORMAL"].as_u64().unwrap() as usize, 3)?;
            let uv = read_f32_vec(&doc, bin, attr["TEXCOORD_0"].as_u64().unwrap() as usize, 2)?;
            let v_joints = read_joints(&doc, bin, attr["JOINTS_0"].as_u64().unwrap() as usize)?;
            let wt = read_f32_vec(&doc, bin, attr["WEIGHTS_0"].as_u64().unwrap() as usize, 4)?;
            let indices = read_u32_indices(&doc, bin, prim["indices"].as_u64().unwrap() as usize)?;
            let vcount = pos.len() / 3;
            let mut vertices = Vec::with_capacity(vcount);
            let mut weights = Vec::with_capacity(vcount);
            for i in 0..vcount {
                vertices.push(Vertex {
                    position: [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]],
                    normal: [norm[i * 3], norm[i * 3 + 1], norm[i * 3 + 2]],
                    uv: [uv[i * 2], uv[i * 2 + 1]],
                });
                weights.push([wt[i * 4], wt[i * 4 + 1], wt[i * 4 + 2], wt[i * 4 + 3]]);
            }
            submeshes.push(SkinnedSubmesh {
                name,
                material: prim["material"].as_u64().unwrap_or(0) as usize,
                rest: MeshData { vertices, indices },
                joints: v_joints,
                weights,
            });
        }

        let mut animations = Vec::new();
        let mut anim_index = HashMap::new();
        if let Some(raw_anims) = doc["animations"].as_array() {
            for a in raw_anims {
                let name = a["name"].as_str().unwrap_or("anim").to_owned();
                let samplers = a["samplers"].as_array().context("Missing samplers")?;
                let raw_channels = a["channels"].as_array().context("Missing channels")?;

                let mut per_node: HashMap<usize, AnimChannel> = HashMap::new();
                let mut duration = 0.0f32;
                for ch in raw_channels {
                    let target = &ch["target"];
                    let Some(node) = target["node"].as_u64().map(|v| v as usize) else {
                        continue;
                    };
                    let path = target["path"].as_str().unwrap_or("");
                    let s_idx = ch["sampler"].as_u64().context("Missing sampler")? as usize;
                    let sampler = &samplers[s_idx];
                    let times = read_f32_vec(
                        &doc,
                        bin,
                        sampler["input"].as_u64().context("Missing input")? as usize,
                        1,
                    )?;
                    if let Some(&t_last) = times.last() {
                        duration = duration.max(t_last);
                    }
                    let out_acc = sampler["output"].as_u64().context("Missing output")? as usize;
                    let entry = per_node.entry(node).or_insert_with(|| AnimChannel {
                        node,
                        times: times.clone(),
                        translations: None,
                        rotations: None,
                        scales: None,
                    });
                    match path {
                        "translation" => {
                            let f = read_f32_vec(&doc, bin, out_acc, 3)?;
                            entry.translations = Some(
                                f.as_chunks::<3>()
                                    .0
                                    .iter()
                                    .copied()
                                    .map(Vec3::from_array)
                                    .collect(),
                            );
                        }
                        "rotation" => {
                            let f = read_f32_vec(&doc, bin, out_acc, 4)?;
                            entry.rotations = Some(
                                f.as_chunks::<4>()
                                    .0
                                    .iter()
                                    .map(|c| Quat::from_xyzw(c[0], c[1], c[2], c[3]).normalize())
                                    .collect(),
                            );
                        }
                        "scale" => {
                            let f = read_f32_vec(&doc, bin, out_acc, 3)?;
                            entry.scales = Some(
                                f.as_chunks::<3>()
                                    .0
                                    .iter()
                                    .copied()
                                    .map(Vec3::from_array)
                                    .collect(),
                            );
                        }
                        _ => {}
                    }
                }
                anim_index.insert(name.clone(), animations.len());
                animations.push(AnimationClip {
                    name,
                    duration: duration.max(1.0 / 60.0),
                    channels: per_node.into_values().collect(),
                });
            }
        }

        let rif_node = nodes.iter().position(|n| n.name == "Rif");
        let pbody_node = nodes.iter().position(|n| n.name == "PBody");

        Ok(Self {
            nodes,
            order,
            joints,
            inverse_bind,
            submeshes,
            animations,
            anim_index,
            rif_node,
            pbody_node,
        })
    }

    pub fn anim_id(&self, name: &str) -> usize {
        self.anim_index.get(name).copied().unwrap_or(0)
    }

    /// Evaluates blended skeletal pose into `skin_mats` (one Mat4 per joint) and returns `PBody` local-view transform.
    pub fn evaluate_pose(
        &self,
        anim_a: usize,
        time_a: f32,
        anim_b: Option<(usize, f32, f32)>,
        slide_lock_back: bool,
        skin_mats: &mut Vec<Mat4>,
    ) -> Mat4 {
        let count = self.nodes.len();
        let mut trs: Vec<(Vec3, Quat, Vec3)> = self
            .nodes
            .iter()
            .map(|n| (n.translation, n.rotation, n.scale))
            .collect();

        if let Some(clip) = self.animations.get(anim_a) {
            for ch in &clip.channels {
                if let Some(v) = &ch.translations {
                    trs[ch.node].0 = sample_vec3(&ch.times, v, time_a);
                }
                if let Some(v) = &ch.rotations {
                    trs[ch.node].1 = sample_quat(&ch.times, v, time_a);
                }
                if let Some(v) = &ch.scales {
                    trs[ch.node].2 = sample_vec3(&ch.times, v, time_a);
                }
            }
        }

        if let Some((b_idx, time_b, weight_b)) = anim_b {
            let w = weight_b.clamp(0.0, 1.0);
            if w > 0.001 {
                if let Some(clip) = self.animations.get(b_idx) {
                    for ch in &clip.channels {
                        if let Some(v) = &ch.translations {
                            let target = sample_vec3(&ch.times, v, time_b);
                            trs[ch.node].0 = trs[ch.node].0.lerp(target, w);
                        }
                        if let Some(v) = &ch.rotations {
                            let target = sample_quat(&ch.times, v, time_b);
                            trs[ch.node].1 = trs[ch.node].1.slerp(target, w);
                        }
                        if let Some(v) = &ch.scales {
                            let target = sample_vec3(&ch.times, v, time_b);
                            trs[ch.node].2 = trs[ch.node].2.lerp(target, w);
                        }
                    }
                }
            }
        }

        if slide_lock_back {
            if let Some(rif) = self.rif_node {
                // Slide rail blowback offset in PBody space when magazine is empty.
                trs[rif].0 += Vec3::new(-0.0007, -0.014, -0.048);
            }
        }

        let mut world = vec![Mat4::IDENTITY; count];
        for &idx in &self.order {
            let (t, r, s) = trs[idx];
            let local = Mat4::from_scale_rotation_translation(s, r, t);
            world[idx] = match self.nodes[idx].parent {
                Some(p) => world[p] * local,
                None => local,
            };
        }

        skin_mats.clear();
        for (ji, &node_idx) in self.joints.iter().enumerate() {
            skin_mats.push(world[node_idx] * self.inverse_bind[ji]);
        }

        self.pbody_node.map(|i| world[i]).unwrap_or(Mat4::IDENTITY)
    }

    pub fn skin_submesh(&self, sub_idx: usize, skin_mats: &[Mat4], out: &mut MeshData) {
        let sub = &self.submeshes[sub_idx];
        out.vertices.resize(
            sub.rest.vertices.len(),
            Vertex {
                position: [0.0; 3],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0; 2],
            },
        );
        if out.indices.len() != sub.rest.indices.len() {
            out.indices = sub.rest.indices.clone();
        }
        for i in 0..sub.rest.vertices.len() {
            let rv = &sub.rest.vertices[i];
            let j = sub.joints[i];
            let w = sub.weights[i];
            let m = skin_mats[j[0] as usize] * w[0]
                + skin_mats[j[1] as usize] * w[1]
                + skin_mats[j[2] as usize] * w[2]
                + skin_mats[j[3] as usize] * w[3];
            let pos = m.transform_point3(Vec3::from(rv.position));
            let nmat = Mat3::from_mat4(m);
            let norm = (nmat * Vec3::from(rv.normal)).normalize_or_zero();
            out.vertices[i] = Vertex {
                position: pos.to_array(),
                normal: norm.to_array(),
                uv: rv.uv,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_skins_fps_pistol_glb() {
        let model = RiggedModel::load(&crate::asset_dir().join("models/fps_pistol.glb"))
            .expect("fps_pistol.glb should load");
        assert_eq!(model.submeshes.len(), 4);
        assert!(model.animations.len() >= 7);
        let mut mats = Vec::new();
        let pbody = model.evaluate_pose(model.anim_id("idle"), 0.0, None, false, &mut mats);
        assert!(pbody.is_finite());
        let mut skinned = MeshData::default();
        for i in 0..model.submeshes.len() {
            model.skin_submesh(i, &mats, &mut skinned);
            assert!(!skinned.vertices.is_empty());
            assert!(skinned
                .vertices
                .iter()
                .all(|v| Vec3::from(v.position).is_finite() && Vec3::from(v.normal).is_finite()));
        }
    }
}
