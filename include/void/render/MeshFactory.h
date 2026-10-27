#pragma once

#include "void/rhi/RHI.h"

#include <glm/glm.hpp>

namespace vd::mesh {

rhi::MeshData box(glm::vec3 halfExtents);
rhi::MeshData sphere(float radius, int segments = 24, int rings = 16);
rhi::MeshData cylinder(float radius, float height, int segments = 24);
rhi::MeshData cone(float radius, float height, int segments = 24);
rhi::MeshData plane(float width, float depth);

} // namespace vd::mesh
