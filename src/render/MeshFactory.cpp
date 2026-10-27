#include "void/render/MeshFactory.h"

#include <cmath>

namespace vd::mesh {
namespace {
constexpr float kPi = 3.14159265358979323846f;

void quad(rhi::MeshData& mesh, glm::vec3 a, glm::vec3 b, glm::vec3 c, glm::vec3 d,
          glm::vec3 normal, glm::vec2 uvA, glm::vec2 uvB, glm::vec2 uvC, glm::vec2 uvD) {
    const u32 base = static_cast<u32>(mesh.vertices.size());
    mesh.vertices.push_back({a, normal, uvA});
    mesh.vertices.push_back({b, normal, uvB});
    mesh.vertices.push_back({c, normal, uvC});
    mesh.vertices.push_back({d, normal, uvD});
    mesh.indices.insert(mesh.indices.end(), {base, base + 1, base + 2, base, base + 2, base + 3});
}
} // namespace

rhi::MeshData box(glm::vec3 h) {
    rhi::MeshData mesh;
    const glm::vec2 uv0{0, 1}, uv1{1, 1}, uv2{1, 0}, uv3{0, 0};
    // +X
    quad(mesh, {h.x, -h.y, h.z}, {h.x, -h.y, -h.z}, {h.x, h.y, -h.z}, {h.x, h.y, h.z},
         {1, 0, 0}, uv0, uv1, uv2, uv3);
    // -X
    quad(mesh, {-h.x, -h.y, -h.z}, {-h.x, -h.y, h.z}, {-h.x, h.y, h.z}, {-h.x, h.y, -h.z},
         {-1, 0, 0}, uv0, uv1, uv2, uv3);
    // +Y
    quad(mesh, {-h.x, h.y, h.z}, {h.x, h.y, h.z}, {h.x, h.y, -h.z}, {-h.x, h.y, -h.z},
         {0, 1, 0}, uv0, uv1, uv2, uv3);
    // -Y
    quad(mesh, {-h.x, -h.y, -h.z}, {h.x, -h.y, -h.z}, {h.x, -h.y, h.z}, {-h.x, -h.y, h.z},
         {0, -1, 0}, uv0, uv1, uv2, uv3);
    // +Z
    quad(mesh, {-h.x, -h.y, h.z}, {h.x, -h.y, h.z}, {h.x, h.y, h.z}, {-h.x, h.y, h.z},
         {0, 0, 1}, uv0, uv1, uv2, uv3);
    // -Z
    quad(mesh, {h.x, -h.y, -h.z}, {-h.x, -h.y, -h.z}, {-h.x, h.y, -h.z}, {h.x, h.y, -h.z},
         {0, 0, -1}, uv0, uv1, uv2, uv3);
    return mesh;
}

rhi::MeshData sphere(float radius, int segments, int rings) {
    rhi::MeshData mesh;
    for (int y = 0; y <= rings; ++y) {
        const float v = static_cast<float>(y) / rings;
        const float phi = v * kPi;
        for (int x = 0; x <= segments; ++x) {
            const float u = static_cast<float>(x) / segments;
            const float theta = u * kPi * 2.0f;
            const glm::vec3 n{std::sin(phi) * std::cos(theta), std::cos(phi),
                              std::sin(phi) * std::sin(theta)};
            mesh.vertices.push_back({n * radius, n, {u, v}});
        }
    }
    for (int y = 0; y < rings; ++y) {
        for (int x = 0; x < segments; ++x) {
            const u32 a = static_cast<u32>(y * (segments + 1) + x);
            const u32 b = a + static_cast<u32>(segments + 1);
            mesh.indices.insert(mesh.indices.end(), {a, b, a + 1, b, b + 1, a + 1});
        }
    }
    return mesh;
}

rhi::MeshData cylinder(float radius, float height, int segments) {
    rhi::MeshData mesh;
    const float half = height * 0.5f;
    for (int i = 0; i <= segments; ++i) {
        const float u = static_cast<float>(i) / segments;
        const float a = u * kPi * 2.0f;
        const float x = std::cos(a) * radius;
        const float z = std::sin(a) * radius;
        const glm::vec3 n{std::cos(a), 0, std::sin(a)};
        mesh.vertices.push_back({{x, -half, z}, n, {u, 1}});
        mesh.vertices.push_back({{x, half, z}, n, {u, 0}});
    }
    for (int i = 0; i < segments; ++i) {
        const u32 a = static_cast<u32>(i * 2);
        mesh.indices.insert(mesh.indices.end(), {a, a + 2, a + 1, a + 1, a + 2, a + 3});
    }
    const u32 bottomCenter = static_cast<u32>(mesh.vertices.size());
    mesh.vertices.push_back({{0, -half, 0}, {0, -1, 0}, {0.5f, 0.5f}});
    const u32 bottomStart = static_cast<u32>(mesh.vertices.size());
    for (int i = 0; i < segments; ++i) {
        const float a = (static_cast<float>(i) / segments) * kPi * 2.0f;
        mesh.vertices.push_back({{std::cos(a) * radius, -half, std::sin(a) * radius},
                                 {0, -1, 0},
                                 {0.5f + std::cos(a) * 0.5f, 0.5f + std::sin(a) * 0.5f}});
    }
    for (int i = 0; i < segments; ++i) {
        mesh.indices.insert(mesh.indices.end(),
                            {bottomCenter, bottomStart + static_cast<u32>(i),
                             bottomStart + static_cast<u32>((i + 1) % segments)});
    }
    const u32 topCenter = static_cast<u32>(mesh.vertices.size());
    mesh.vertices.push_back({{0, half, 0}, {0, 1, 0}, {0.5f, 0.5f}});
    const u32 topStart = static_cast<u32>(mesh.vertices.size());
    for (int i = 0; i < segments; ++i) {
        const float a = (static_cast<float>(i) / segments) * kPi * 2.0f;
        mesh.vertices.push_back({{std::cos(a) * radius, half, std::sin(a) * radius},
                                 {0, 1, 0},
                                 {0.5f + std::cos(a) * 0.5f, 0.5f + std::sin(a) * 0.5f}});
    }
    for (int i = 0; i < segments; ++i) {
        mesh.indices.insert(mesh.indices.end(),
                            {topCenter, topStart + static_cast<u32>((i + 1) % segments),
                             topStart + static_cast<u32>(i)});
    }
    return mesh;
}

rhi::MeshData cone(float radius, float height, int segments) {
    rhi::MeshData mesh;
    const float half = height * 0.5f;
    const u32 apex = 0;
    mesh.vertices.push_back({{0, half, 0}, {0, 1, 0}, {0.5f, 0}});
    const u32 ringStart = 1;
    for (int i = 0; i <= segments; ++i) {
        const float u = static_cast<float>(i) / segments;
        const float a = u * kPi * 2.0f;
        const glm::vec3 n{std::cos(a) * 0.7f, 0.4f, std::sin(a) * 0.7f};
        mesh.vertices.push_back({{std::cos(a) * radius, -half, std::sin(a) * radius}, n, {u, 1}});
    }
    for (int i = 0; i < segments; ++i) {
        mesh.indices.insert(mesh.indices.end(),
                            {apex, ringStart + static_cast<u32>(i),
                             ringStart + static_cast<u32>(i + 1)});
    }
    const u32 center = static_cast<u32>(mesh.vertices.size());
    mesh.vertices.push_back({{0, -half, 0}, {0, -1, 0}, {0.5f, 0.5f}});
    const u32 base = static_cast<u32>(mesh.vertices.size());
    for (int i = 0; i < segments; ++i) {
        const float a = (static_cast<float>(i) / segments) * kPi * 2.0f;
        mesh.vertices.push_back({{std::cos(a) * radius, -half, std::sin(a) * radius},
                                 {0, -1, 0},
                                 {0.5f + std::cos(a) * 0.5f, 0.5f + std::sin(a) * 0.5f}});
    }
    for (int i = 0; i < segments; ++i) {
        mesh.indices.insert(mesh.indices.end(),
                            {center, base + static_cast<u32>(i),
                             base + static_cast<u32>((i + 1) % segments)});
    }
    return mesh;
}

rhi::MeshData plane(float width, float depth) {
    rhi::MeshData mesh;
    const float hw = width * 0.5f;
    const float hd = depth * 0.5f;
    quad(mesh, {-hw, 0, hd}, {hw, 0, hd}, {hw, 0, -hd}, {-hw, 0, -hd},
         {0, 1, 0}, {0, 1}, {1, 1}, {1, 0}, {0, 0});
    return mesh;
}

} // namespace vd::mesh
