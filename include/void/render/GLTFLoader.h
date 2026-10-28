#pragma once

#include "void/rhi/RHI.h"

#include <glm/glm.hpp>

#include <string>

namespace vd::gltf {

struct Model {
    rhi::MeshData mesh;
    rhi::ImageData baseColor;
    glm::vec3 baseColorFactor{1.0f};

    bool valid() const { return !mesh.vertices.empty() && !mesh.indices.empty(); }
};

// Minimal GLB 2.0 loader for static meshes with a baked base-color texture.
// It reads embedded buffers/images and ignores animation, skinning and
// extensions the demo does not use. Texture dimensions are capped to keep
// startup memory predictable.
Model loadGLB(const std::string& path, u32 maxTextureSize = 2048);

} // namespace vd::gltf
