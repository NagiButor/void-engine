#include "void/render/GLTFLoader.h"

#include "void/core/Log.h"

#ifdef _MSC_VER
#pragma warning(push)
#pragma warning(disable: 4505)
#endif
#define STB_IMAGE_IMPLEMENTATION
#define STBI_ONLY_PNG
#include <stb_image.h>
#ifdef _MSC_VER
#pragma warning(pop)
#endif

#include <nlohmann/json.hpp>

#include <glm/gtc/matrix_transform.hpp>
#include <glm/gtc/quaternion.hpp>
#include <glm/gtc/type_ptr.hpp>

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <vector>

namespace vd::gltf {
namespace {

using json = nlohmann::json;

struct Glb {
    json document;
    std::vector<u8> bin;
};

u32 readU32(const u8* p) {
    u32 value = 0;
    std::memcpy(&value, p, sizeof(value));
    return value;
}

bool parseGlb(const std::string& path, Glb& out) {
    std::ifstream file(path, std::ios::binary | std::ios::ate);
    if (!file) {
        log::error("GLB: cannot open %s", path.c_str());
        return false;
    }
    const std::streamsize size = file.tellg();
    file.seekg(0);
    std::vector<u8> data(static_cast<size_t>(size));
    file.read(reinterpret_cast<char*>(data.data()), size);
    if (data.size() < 20u || readU32(data.data()) != 0x46546C67u) {
        log::error("GLB: %s is not a binary glTF file", path.c_str());
        return false;
    }

    size_t offset = 12;
    bool haveJson = false;
    while (offset + 8 <= data.size()) {
        const u32 chunkLength = readU32(&data[offset]);
        const u32 chunkType = readU32(&data[offset + 4]);
        offset += 8;
        if (offset + chunkLength > data.size()) break;
        if (chunkType == 0x4E4F534Au) {
            try {
                out.document = json::parse(reinterpret_cast<const char*>(&data[offset]),
                                           reinterpret_cast<const char*>(&data[offset]) + chunkLength);
                haveJson = true;
            } catch (const std::exception& e) {
                log::error("GLB: JSON parse failed: %s", e.what());
                return false;
            }
        } else if (chunkType == 0x004E4942u) {
            out.bin.assign(data.begin() + static_cast<std::ptrdiff_t>(offset),
                           data.begin() + static_cast<std::ptrdiff_t>(offset + chunkLength));
        }
        offset += chunkLength;
    }
    if (!haveJson) {
        log::error("GLB: %s has no JSON chunk", path.c_str());
        return false;
    }
    return true;
}

struct AccessorView {
    const u8* base = nullptr;
    u32 count = 0;
    u32 components = 0;
    u32 componentSize = 0;
    u32 stride = 0;
    u32 componentType = 0;
};

bool accessorView(const Glb& glb, int index, AccessorView& out) {
    const json& accessors = glb.document.at("accessors");
    if (index < 0 || static_cast<size_t>(index) >= accessors.size()) return false;
    const json& accessor = accessors[index];
    if (!accessor.contains("bufferView")) return false;
    const int bufferViewIndex = accessor.at("bufferView").get<int>();
    const json& views = glb.document.at("bufferViews");
    if (bufferViewIndex < 0 || static_cast<size_t>(bufferViewIndex) >= views.size()) return false;
    const json& view = views[bufferViewIndex];
    const u32 viewOffset = view.value("byteOffset", 0u);
    const u32 accessorOffset = accessor.value("byteOffset", 0u);
    const u32 typeComponents = [&]() {
        const std::string type = accessor.value("type", std::string("SCALAR"));
        if (type == "VEC2") return 2u;
        if (type == "VEC3") return 3u;
        if (type == "VEC4") return 4u;
        return 1u;
    }();
    out.componentType = accessor.value("componentType", 0u);
    switch (out.componentType) {
    case 5120: out.componentSize = 1; break; // BYTE
    case 5121: out.componentSize = 1; break; // UNSIGNED_BYTE
    case 5122: out.componentSize = 2; break; // SHORT
    case 5123: out.componentSize = 2; break; // UNSIGNED_SHORT
    case 5125: out.componentSize = 4; break; // UNSIGNED_INT
    case 5126: out.componentSize = 4; break; // FLOAT
    default:
        return false;
    }
    out.count = accessor.value("count", 0u);
    out.components = typeComponents;
    out.stride = view.value("byteStride", 0u);
    if (out.stride == 0) out.stride = out.components * out.componentSize;
    const size_t byteOffset = static_cast<size_t>(viewOffset) + accessorOffset;
    if (byteOffset > glb.bin.size()) return false;
    out.base = glb.bin.data() + byteOffset;
    return true;
}

float readFloatComponent(const u8* p, u32 componentType, bool normalize) {
    switch (componentType) {
    case 5120: {
        std::int8_t value = 0;
        std::memcpy(&value, p, 1);
        return normalize ? std::max(static_cast<float>(value) / 127.0f, -1.0f)
                         : static_cast<float>(value);
    }
    case 5121: {
        return normalize ? static_cast<float>(*p) / 255.0f : static_cast<float>(*p);
    }
    case 5122: {
        std::int16_t value = 0;
        std::memcpy(&value, p, 2);
        return normalize ? std::max(static_cast<float>(value) / 32767.0f, -1.0f)
                         : static_cast<float>(value);
    }
    case 5123: {
        u16 value = 0;
        std::memcpy(&value, p, 2);
        return normalize ? static_cast<float>(value) / 65535.0f : static_cast<float>(value);
    }
    case 5125: {
        u32 value = 0;
        std::memcpy(&value, p, 4);
        return static_cast<float>(value);
    }
    case 5126: {
        float value = 0.0f;
        std::memcpy(&value, p, 4);
        return value;
    }
    default: return 0.0f;
    }
}

u32 readIndexComponent(const u8* p, u32 componentType) {
    switch (componentType) {
    case 5121: return static_cast<u32>(*p);
    case 5123: {
        u16 value = 0;
        std::memcpy(&value, p, 2);
        return value;
    }
    case 5125: {
        u32 value = 0;
        std::memcpy(&value, p, 4);
        return value;
    }
    default: return 0;
    }
}

bool readFloats(const Glb& glb, int index, std::vector<float>& out, u32 expectedComponents,
                bool normalize = false) {
    AccessorView view;
    if (!accessorView(glb, index, view)) return false;
    if (view.components < expectedComponents) return false;
    out.resize(static_cast<size_t>(view.count) * expectedComponents);
    for (u32 i = 0; i < view.count; ++i) {
        const u8* element = view.base + static_cast<size_t>(i) * view.stride;
        for (u32 c = 0; c < expectedComponents; ++c) {
            out[static_cast<size_t>(i) * expectedComponents + c] =
                readFloatComponent(element + c * view.componentSize, view.componentType, normalize);
        }
    }
    return true;
}

bool readIndices(const Glb& glb, int index, std::vector<u32>& out) {
    AccessorView view;
    if (!accessorView(glb, index, view)) return false;
    out.resize(view.count);
    for (u32 i = 0; i < view.count; ++i) {
        const u8* element = view.base + static_cast<size_t>(i) * view.stride;
        out[i] = readIndexComponent(element, view.componentType);
    }
    return true;
}

glm::mat4 nodeMatrix(const json& node) {
    glm::mat4 result(1.0f);
    if (node.contains("matrix") && node["matrix"].is_array()) {
        float values[16]{};
        for (int i = 0; i < 16; ++i) values[i] = node["matrix"][i].get<float>();
        result = glm::make_mat4(values);
        return result;
    }
    glm::vec3 translation(0.0f);
    glm::quat rotation(1.0f, 0.0f, 0.0f, 0.0f);
    glm::vec3 scale(1.0f);
    if (node.contains("translation")) {
        translation = {node["translation"][0].get<float>(), node["translation"][1].get<float>(),
                       node["translation"][2].get<float>()};
    }
    if (node.contains("rotation")) {
        rotation = glm::quat(node["rotation"][3].get<float>(), node["rotation"][0].get<float>(),
                             node["rotation"][1].get<float>(), node["rotation"][2].get<float>());
    }
    if (node.contains("scale")) {
        scale = {node["scale"][0].get<float>(), node["scale"][1].get<float>(),
                 node["scale"][2].get<float>()};
    }
    return glm::translate(glm::mat4(1.0f), translation) * glm::mat4_cast(rotation) *
           glm::scale(glm::mat4(1.0f), scale);
}

rhi::ImageData resizeImage(const rhi::ImageData& source, u32 maxSize) {
    if (source.width <= maxSize && source.height <= maxSize) return source;
    const float scale = std::min(static_cast<float>(maxSize) / source.width,
                                 static_cast<float>(maxSize) / source.height);
    const u32 width = std::max(1u, static_cast<u32>(static_cast<float>(source.width) * scale));
    const u32 height = std::max(1u, static_cast<u32>(static_cast<float>(source.height) * scale));
    rhi::ImageData out;
    out.width = width;
    out.height = height;
    out.srgb = source.srgb;
    out.pixels.resize(static_cast<size_t>(width) * height * 4);
    for (u32 y = 0; y < height; ++y) {
        const float sy = (static_cast<float>(y) + 0.5f) / scale - 0.5f;
        const u32 y0 = static_cast<u32>(std::clamp(sy, 0.0f, static_cast<float>(source.height - 1)));
        const u32 y1 = std::min(y0 + 1, source.height - 1);
        const float fy = std::clamp(sy - static_cast<float>(y0), 0.0f, 1.0f);
        for (u32 x = 0; x < width; ++x) {
            const float sx = (static_cast<float>(x) + 0.5f) / scale - 0.5f;
            const u32 x0 = static_cast<u32>(std::clamp(sx, 0.0f, static_cast<float>(source.width - 1)));
            const u32 x1 = std::min(x0 + 1, source.width - 1);
            const float fx = std::clamp(sx - static_cast<float>(x0), 0.0f, 1.0f);
            for (u32 c = 0; c < 4; ++c) {
                const auto at = [&](u32 px, u32 py) {
                    return static_cast<float>(source.pixels[(static_cast<size_t>(py) * source.width + px) * 4 + c]);
                };
                const float top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
                const float bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
                out.pixels[(static_cast<size_t>(y) * width + x) * 4 + c] =
                    static_cast<u8>(std::clamp(top + (bottom - top) * fy, 0.0f, 255.0f));
            }
        }
    }
    return out;
}

bool loadBaseColor(const Glb& glb, const json& material, rhi::ImageData& out, u32 maxSize) {
    if (!material.contains("pbrMetallicRoughness")) return false;
    const json& pbr = material["pbrMetallicRoughness"];
    if (!pbr.contains("baseColorTexture")) return false;
    const int textureIndex = pbr["baseColorTexture"].value("index", -1);
    if (textureIndex < 0) return false;
    const json& textures = glb.document.at("textures");
    if (static_cast<size_t>(textureIndex) >= textures.size()) return false;
    const int sourceIndex = textures[textureIndex].value("source", -1);
    const json& images = glb.document.at("images");
    if (sourceIndex < 0 || static_cast<size_t>(sourceIndex) >= images.size()) return false;
    const json& image = images[sourceIndex];
    if (!image.contains("bufferView")) return false;
    const int viewIndex = image["bufferView"].get<int>();
    const json& views = glb.document.at("bufferViews");
    if (viewIndex < 0 || static_cast<size_t>(viewIndex) >= views.size()) return false;
    const json& view = views[viewIndex];
    const size_t offset = view.value("byteOffset", 0u);
    const size_t length = view.value("byteLength", 0u);
    if (offset + length > glb.bin.size()) return false;

    int width = 0;
    int height = 0;
    int channels = 0;
    stbi_uc* pixels = stbi_load_from_memory(glb.bin.data() + offset, static_cast<int>(length),
                                            &width, &height, &channels, 4);
    if (!pixels) {
        log::error("GLB: base-color image decode failed: %s", stbi_failure_reason());
        return false;
    }
    out.width = static_cast<u32>(width);
    out.height = static_cast<u32>(height);
    out.srgb = true;
    out.pixels.assign(pixels, pixels + static_cast<size_t>(width) * height * 4);
    stbi_image_free(pixels);
    out = resizeImage(out, maxSize);
    log::info("GLB: base-color texture %ux%u (source %dx%d)", out.width, out.height, width, height);
    return true;
}

void appendPrimitive(const Glb& glb, const json& primitive, const glm::mat4& transform,
                     Model& model, bool& haveTexture, u32 maxTextureSize) {
    if (!primitive.contains("attributes")) return;
    const json& attributes = primitive["attributes"];
    if (!attributes.contains("POSITION")) return;
    const int positionAccessor = attributes["POSITION"].get<int>();
    std::vector<float> positions;
    if (!readFloats(glb, positionAccessor, positions, 3, true)) return;
    const u32 vertexCount = static_cast<u32>(positions.size() / 3);

    std::vector<float> normals;
    if (attributes.contains("NORMAL")) {
        readFloats(glb, attributes["NORMAL"].get<int>(), normals, 3, true);
    }
    std::vector<float> uvs;
    if (attributes.contains("TEXCOORD_0")) {
        readFloats(glb, attributes["TEXCOORD_0"].get<int>(), uvs, 2, true);
    }

    const u32 baseVertex = static_cast<u32>(model.mesh.vertices.size());
    model.mesh.vertices.resize(baseVertex + vertexCount);
    const glm::mat3 normalMatrix = glm::transpose(glm::inverse(glm::mat3(transform)));
    for (u32 i = 0; i < vertexCount; ++i) {
        rhi::Vertex& vertex = model.mesh.vertices[baseVertex + i];
        const glm::vec4 position(positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2], 1.0f);
        vertex.position = glm::vec3(transform * position);
        if (normals.size() >= static_cast<size_t>(i) * 3 + 3) {
            vertex.normal = glm::normalize(
                normalMatrix * glm::vec3(normals[i * 3], normals[i * 3 + 1], normals[i * 3 + 2]));
        } else {
            vertex.normal = glm::vec3(0.0f, 1.0f, 0.0f);
        }
        if (uvs.size() >= static_cast<size_t>(i) * 2 + 2) {
            vertex.uv = {uvs[i * 2], uvs[i * 2 + 1]};
        }
    }

    std::vector<u32> indices;
    if (primitive.contains("indices")) {
        readIndices(glb, primitive["indices"].get<int>(), indices);
    } else {
        indices.resize(vertexCount);
        for (u32 i = 0; i < vertexCount; ++i) indices[i] = i;
    }
    for (u32 index : indices) model.mesh.indices.push_back(baseVertex + index);

    if (!haveTexture && primitive.contains("material") && glb.document.contains("materials")) {
        const json& materials = glb.document["materials"];
        const int materialIndex = primitive["material"].get<int>();
        if (materialIndex >= 0 && static_cast<size_t>(materialIndex) < materials.size()) {
            const json& material = materials[materialIndex];
            if (material.contains("pbrMetallicRoughness") &&
                material["pbrMetallicRoughness"].contains("baseColorFactor")) {
                const json& factor = material["pbrMetallicRoughness"]["baseColorFactor"];
                if (factor.is_array() && factor.size() >= 3) {
                    model.baseColorFactor = {factor[0].get<float>(), factor[1].get<float>(),
                                             factor[2].get<float>()};
                }
            }
            haveTexture = loadBaseColor(glb, material, model.baseColor, maxTextureSize);
        }
    }
}

void traverseNodes(const Glb& glb, int nodeIndex, const glm::mat4& parent, Model& model,
                   bool& haveTexture, u32 maxTextureSize) {
    const json& nodes = glb.document.at("nodes");
    if (nodeIndex < 0 || static_cast<size_t>(nodeIndex) >= nodes.size()) return;
    const json& node = nodes[nodeIndex];
    const glm::mat4 transform = parent * nodeMatrix(node);
    if (node.contains("mesh")) {
        const int meshIndex = node["mesh"].get<int>();
        const json& meshes = glb.document.at("meshes");
        if (meshIndex >= 0 && static_cast<size_t>(meshIndex) < meshes.size()) {
            for (const auto& primitive : meshes[meshIndex]["primitives"]) {
                appendPrimitive(glb, primitive, transform, model, haveTexture, maxTextureSize);
            }
        }
    }
    if (node.contains("children")) {
        for (const auto& child : node["children"]) {
            traverseNodes(glb, child.get<int>(), transform, model, haveTexture, maxTextureSize);
        }
    }
}

} // namespace

Model loadGLB(const std::string& path, u32 maxTextureSize) {
    Model model;
    Glb glb;
    if (!parseGlb(path, glb)) return model;
    try {
    if (!glb.document.contains("nodes") || !glb.document.contains("meshes")) {
        log::error("GLB: %s has no nodes or meshes", path.c_str());
        return model;
    }

    bool haveTexture = false;
    if (glb.document.contains("scenes") && !glb.document["scenes"].empty()) {
        const json& scene = glb.document["scenes"][glb.document.value("scene", 0)];
        if (scene.contains("nodes")) {
            for (const auto& node : scene["nodes"]) {
                traverseNodes(glb, node.get<int>(), glm::mat4(1.0f), model, haveTexture, maxTextureSize);
            }
        }
    } else {
        const json& nodes = glb.document["nodes"];
        for (size_t i = 0; i < nodes.size(); ++i) {
            traverseNodes(glb, static_cast<int>(i), glm::mat4(1.0f), model, haveTexture, maxTextureSize);
        }
    }

    log::info("GLB: %s -> %u vertices, %u triangles, texture %s", path.c_str(),
              static_cast<unsigned>(model.mesh.vertices.size()),
              static_cast<unsigned>(model.mesh.indices.size() / 3),
              haveTexture ? "loaded" : "missing");
    } catch (const std::exception& e) {
        log::error("GLB: %s parse error: %s", path.c_str(), e.what());
        return {};
    }
    return model;
}

} // namespace vd::gltf
