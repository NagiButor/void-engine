#pragma once

#include "void/core/Types.h"

#include <array>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

struct SDL_Window;

namespace vd::rhi {

enum class Api { Auto, DX12, Vulkan };

const char* apiName(Api api);

struct DeviceDesc {
    SDL_Window* window = nullptr;
    u32 width = 0;
    u32 height = 0;
    bool vsync = true;
    bool validation = false;
    int adapter = -1;
    const char* appName = "V.O.I.D. Engine";
};

struct MeshHandle {
    u32 id = 0;
    explicit operator bool() const { return id != 0; }
};

struct ImageData {
    u32 width = 0;
    u32 height = 0;
    std::vector<u8> pixels; // RGBA8
    bool srgb = true;
};

struct Vertex {
    glm::vec3 position;
    glm::vec3 normal;
    glm::vec2 uv;
};

struct MeshData {
    std::vector<Vertex> vertices;
    std::vector<u32> indices;
};

struct DrawItem {
    MeshHandle mesh;
    glm::mat4 transform{1.0f};
    u32 textureLayer = 0;
    glm::vec4 baseColor{1.0f};
    float roughness = 0.6f;
    float metallic = 0.0f;
    float emissive = 0.0f;
    glm::vec2 uvScale{1.0f, 1.0f};
    bool castShadow = true;
    bool viewmodel = false;
};

struct LineVertex {
    glm::vec3 position;
    glm::vec4 color;
};

struct UIVertex {
    glm::vec2 position;
    glm::vec2 uv;
    glm::vec4 color;
};

struct FrameData {
    glm::mat4 viewProj{1.0f};
    glm::mat4 invViewProj{1.0f};
    glm::mat4 shadowMatrix{1.0f};
    glm::vec4 cameraPos{0.0f};
    glm::vec4 sunDirection{0.0f, 1.0f, 0.0f, 0.0f};
    glm::vec4 sunColor{1.0f, 1.0f, 1.0f, 1.0f};
    glm::vec4 ambient{0.2f, 0.22f, 0.25f, 1.0f};
    glm::vec4 fog{0.55f, 0.55f, 0.52f, 0.01f};
    std::array<glm::vec4, 3> pointPos{};
    std::array<glm::vec4, 3> pointColor{};
    glm::vec4 skyTop{0.1f, 0.2f, 0.35f, 1.0f};
    glm::vec4 skyHorizon{0.7f, 0.65f, 0.55f, 1.0f};
    glm::vec4 params{1.0f, 0.0f, 0.0f, 0.0f}; // exposure, time, y-flip, unused
};

struct DrawList {
    std::vector<DrawItem> items;
    std::vector<LineVertex> lines;
    glm::mat4 shadowViewProj{1.0f};
};

struct UIList {
    std::vector<UIVertex> vertices;
    u32 textureWidth = 0;
    u32 textureHeight = 0;
};

struct Stats {
    u32 drawCalls = 0;
    u32 triangles = 0;
    u32 lines = 0;
    float frameMs = 0.0f;
};

class Device {
public:
    virtual ~Device() = default;

    virtual bool initialize(const DeviceDesc& desc) = 0;
    virtual void shutdown() = 0;
    virtual void resize(u32 width, u32 height) = 0;
    virtual void waitIdle() = 0;

    virtual MeshHandle createMesh(const MeshData& mesh) = 0;
    virtual bool createTextureArray(const std::vector<ImageData>& layers) = 0;
    virtual bool createFontTexture(const ImageData& image) = 0;

    virtual void render(const FrameData& frame, const DrawList& draws, const UIList& ui) = 0;

    virtual Api api() const = 0;
    virtual const char* adapterName() const = 0;
    virtual const Stats& stats() const = 0;
};

std::unique_ptr<Device> createDX12Device();
std::unique_ptr<Device> createVulkanDevice();

} // namespace vd::rhi
