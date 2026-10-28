#include "demo/Demo.h"

#include "void/audio/Audio.h"
#include "void/core/Log.h"
#include "void/core/Paths.h"
#include "void/physics/PhysicsWorld.h"
#include "void/platform/Input.h"
#include "void/platform/Window.h"
#include "void/render/GLTFLoader.h"
#include "void/render/MeshFactory.h"
#include "void/render/Textures.h"
#include "void/rhi/RHI.h"
#include "void/scene/Camera.h"
#include "void/ui/Font.h"

#include <SDL3/SDL.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

namespace vd::demo {
namespace {

struct Options {
    rhi::Api api = rhi::Api::Auto;
    int adapter = -1;
    u32 width = 1600;
    u32 height = 900;
    bool vsync = true;
    bool validation = false;
    bool fullscreen = false;
    bool noPrompt = false;
    bool autostart = false;
    bool selftest = false;
    bool showcase = false;
    u32 smokeFrames = 0;
};

Options parseOptions(int argc, char** argv) {
    Options options;
    for (int i = 1; i < argc; ++i) {
        const char* arg = argv[i];
        if (std::strncmp(arg, "--rhi=", 6) == 0) {
            const char* value = arg + 6;
            if (std::strcmp(value, "dx12") == 0) options.api = rhi::Api::DX12;
            else if (std::strcmp(value, "vulkan") == 0) options.api = rhi::Api::Vulkan;
            else options.api = rhi::Api::Auto;
        } else if (std::strncmp(arg, "--adapter=", 10) == 0) {
            options.adapter = std::atoi(arg + 10);
        } else if (std::strncmp(arg, "--width=", 8) == 0) {
            options.width = static_cast<u32>(std::max(640, std::atoi(arg + 8)));
        } else if (std::strncmp(arg, "--height=", 9) == 0) {
            options.height = static_cast<u32>(std::max(360, std::atoi(arg + 9)));
        } else if (std::strncmp(arg, "--smoke=", 8) == 0) {
            options.smokeFrames = static_cast<u32>(std::max(0, std::atoi(arg + 8)));
        } else if (std::strcmp(arg, "--novsync") == 0) {
            options.vsync = false;
        } else if (std::strcmp(arg, "--validation") == 0) {
            options.validation = true;
        } else if (std::strcmp(arg, "--fullscreen") == 0) {
            options.fullscreen = true;
        } else if (std::strcmp(arg, "--no-prompt") == 0) {
            options.noPrompt = true;
        } else if (std::strcmp(arg, "--autostart") == 0) {
            options.autostart = true;
        } else if (std::strcmp(arg, "--selftest") == 0) {
            options.selftest = true;
        } else if (std::strcmp(arg, "--showcase") == 0) {
            options.showcase = true;
        }
    }
    return options;
}

rhi::Api chooseApi(Options& options) {
#if defined(VOID_WITH_DX12) && defined(VOID_WITH_VULKAN)
    if (options.api == rhi::Api::Auto && !options.noPrompt) {
        const SDL_MessageBoxButtonData buttons[] = {
            {SDL_MESSAGEBOX_BUTTON_RETURNKEY_DEFAULT, 1, "DirectX 12"},
            {0, 2, "Vulkan"},
            {SDL_MESSAGEBOX_BUTTON_ESCAPEKEY_DEFAULT, 0, "Cancel"},
        };
        const SDL_MessageBoxData box{
            SDL_MESSAGEBOX_INFORMATION, nullptr, "V.O.I.D. Engine",
            "Select a graphics backend for this session.",
            SDL_arraysize(buttons), buttons, nullptr,
        };
        int result = 0;
        if (SDL_ShowMessageBox(&box, &result) && result != 0) {
            options.api = result == 1 ? rhi::Api::DX12 : rhi::Api::Vulkan;
        } else if (result == 0) {
            options.api = rhi::Api::DX12;
        }
    }
#endif
    if (options.api == rhi::Api::Auto) {
#ifdef VOID_WITH_DX12
        options.api = rhi::Api::DX12;
#elif defined(VOID_WITH_VULKAN)
        options.api = rhi::Api::Vulkan;
#endif
    }
    return options.api;
}

constexpr float kPi = 3.14159265358979323846f;
constexpr float kFixedStep = 1.0f / 60.0f;
constexpr u32 kPlayerTag = 1;
constexpr u32 kSpawnLimit = 34;

glm::vec4 rgba(float r, float g, float b, float a = 1.0f) { return {r, g, b, a}; }

glm::mat4 trs(glm::vec3 position, glm::quat rotation, glm::vec3 scale = glm::vec3(1.0f)) {
    return glm::translate(glm::mat4(1.0f), position) * glm::mat4_cast(rotation) *
           glm::scale(glm::mat4(1.0f), scale);
}

void addRect(rhi::UIList& list, const Font& font, float x, float y, float w, float h,
             glm::vec4 color) {
    const glm::vec2 uv = font.solidUv();
    list.vertices.push_back({{x, y}, uv, color});
    list.vertices.push_back({{x + w, y}, uv, color});
    list.vertices.push_back({{x + w, y + h}, uv, color});
    list.vertices.push_back({{x, y}, uv, color});
    list.vertices.push_back({{x + w, y + h}, uv, color});
    list.vertices.push_back({{x, y + h}, uv, color});
}

void addBorder(rhi::UIList& list, const Font& font, float x, float y, float w, float h,
               float thickness, glm::vec4 color) {
    addRect(list, font, x, y, w, thickness, color);
    addRect(list, font, x, y + h - thickness, w, thickness, color);
    addRect(list, font, x, y, thickness, h, color);
    addRect(list, font, x + w - thickness, y, thickness, h, color);
}

void addText(rhi::UIList& list, const Font& font, float x, float y, float size,
             glm::vec4 color, const std::string& text) {
    font.draw(list, x, y, size, color, text);
}

void addTextCentered(rhi::UIList& list, const Font& font, float cx, float y, float size,
                     glm::vec4 color, const std::string& text) {
    font.drawCentered(list, cx, y, size, color, text);
}

enum class PropKind : u32 {
    Crate, Barrel, Sphere, Plank, Brick, Can, Domino, Cone, Pallet, BigPlank, Count
};

struct PropKindInfo {
    const char* label;
    u32 layer;
    glm::vec4 color;
    float roughness;
    float metallic;
    float friction;
    float restitution;
    float density;
    float halfX;
    float halfY;
    float halfZ;
};

struct Prop {
    PropKind kind = PropKind::Crate;
    physics::Body body;
    u32 tag = 0;
    rhi::MeshHandle mesh;
    glm::vec3 spawnPosition{0.0f};
    glm::quat spawnRotation{1, 0, 0, 0};
    bool initial = true;
    rhi::DrawItem draw;
};

struct StaticDraw {
    rhi::MeshHandle mesh;
    glm::mat4 transform{1.0f};
    u32 layer = 3;
    glm::vec4 color{1.0f};
    glm::vec2 uvScale{1.0f};
    float roughness = 0.8f;
    float metallic = 0.0f;
    float emissive = 0.0f;
    bool castShadow = true;
};

struct ViewPart {
    rhi::MeshHandle mesh;
    glm::mat4 local{1.0f};
    glm::vec4 color{0.05f, 0.06f, 0.07f, 1.0f};
    float roughness = 0.4f;
    float metallic = 0.85f;
    float emissive = 0.0f;
    bool unlit = false;
    bool viewTexture = false;
};

enum class State { Menu, Playing, Paused };

class Game {
public:
    Game(Window& window, rhi::Device& device, Options options)
        : m_window(window), m_device(device), m_options(options) {}

    bool initialize();
    void shutdown();
    void startNow() { startGame(); }
    bool selfTest();
    void showcase();
    void setInput(Input* input) { m_input = input; }
    void frame(float rawDt);
    bool ready() const { return m_ready; }

private:
    void buildLevel();
    void buildProps();
    void syncProps();
    void resetScene();
    void startGame();
    void pauseGame();
    void resumeGame();

    void updatePlayer(float dt);
    void updateGrab(float dt);
    void tryGrab();
    void releaseHeld(bool punt);
    void shove();
    void spawnProp(PropKind kind);
    void spawnRandom();
    void removeProp(Prop& prop);
    void updateHover();

    void buildFrame(rhi::FrameData& frame, rhi::DrawList& draws, rhi::UIList& ui);
    void buildMenuUI(rhi::UIList& ui);
    void buildHudUI(rhi::UIList& ui);
    void buildPauseUI(rhi::UIList& ui);
    void buildViewmodel(rhi::DrawList& draws);
    void buildBeam(rhi::DrawList& draws);
    void addToast(const std::string& text);
    void onHit(u32 tagA, u32 tagB, float speed);

    Prop* findProp(u32 tag);
    rhi::MeshHandle makeMesh(const rhi::MeshData& data);
    void addStaticBox(glm::vec3 size, glm::vec3 position, glm::quat rotation,
                      u32 layer, glm::vec2 uvScale, bool collider = true, bool shadow = true,
                      glm::vec4 color = glm::vec4(1.0f), float roughness = 0.8f,
                      float metallic = 0.0f);
    void addStaticVisual(glm::vec3 size, glm::vec3 position, glm::quat rotation,
                         u32 layer, glm::vec2 uvScale, glm::vec4 color = glm::vec4(1.0f));

    Window& m_window;
    rhi::Device& m_device;
    Input* m_input = nullptr;
    Options m_options;
    bool m_ready = false;

    Font m_font;
    AudioKit m_audio;
    Camera m_camera;
    Camera m_menuCamera;
    physics::PhysicsWorld m_physics;
    physics::Body m_player;
    State m_state = State::Menu;
    float m_menuTime = 0.0f;
    float m_timeScale = 1.0f;
    bool m_slowmo = false;
    bool m_muted = false;
    bool m_keepHeld = false;
    float m_accumulator = 0.0f;
    float m_footstepTimer = 0.0f;
    float m_time = 0.0f;
    float m_fps = 60.0f;
    u32 m_fpsFrames = 0;
    float m_fpsTimer = 0.0f;
    u32 m_frameCount = 0;

    rhi::MeshHandle m_manipulatorMesh;
    rhi::MeshHandle m_beamMesh;
    std::array<rhi::MeshHandle, static_cast<size_t>(PropKind::Count)> m_propMeshes{};
    std::vector<StaticDraw> m_staticDraws;
    std::vector<std::unique_ptr<Prop>> m_props;
    std::unordered_map<u32, Prop*> m_propsByTag;
    std::vector<ViewPart> m_viewParts;
    u32 m_nextTag = kPlayerTag + 1;
    u32 m_initialPropCount = 0;

    Prop* m_hover = nullptr;
    Prop* m_held = nullptr;
    physics::ShapeFilter m_heldSavedFilter;
    float m_holdDistance = 2.2f;

    std::string m_toast;
    float m_toastTimer = 0.0f;
    glm::vec3 m_playerSpawn{0.0f, 0.81f, 9.6f};
    glm::vec3 m_sunDir{0.45f, 0.7f, 0.32f};

    std::string m_backend;
};

rhi::MeshHandle Game::makeMesh(const rhi::MeshData& data) {
    rhi::MeshHandle handle = m_device.createMesh(data);
    if (!handle) log::error("Failed to create mesh");
    return handle;
}

bool Game::initialize() {
    const std::string fontPath = Paths::asset("fonts/Roboto-Regular.ttf").string();
    if (!m_font.load(fontPath)) return false;
    rhi::ImageData fontImage;
    fontImage.width = static_cast<u32>(m_font.atlasWidth());
    fontImage.height = static_cast<u32>(m_font.atlasHeight());
    fontImage.pixels = m_font.pixels();
    fontImage.srgb = false;
    if (!m_device.createFontTexture(fontImage)) {
        log::error("Failed to upload font atlas");
        return false;
    }

    auto textures = textures::createLabTextures(fontPath);
    if (!m_device.createTextureArray(textures)) {
        log::error("Failed to upload material texture array");
        return false;
    }

    m_beamMesh = makeMesh(mesh::box({0.012f, 0.012f, 0.5f}));
    m_propMeshes[static_cast<size_t>(PropKind::Crate)] = makeMesh(mesh::box({0.45f, 0.45f, 0.45f}));
    m_propMeshes[static_cast<size_t>(PropKind::Barrel)] = makeMesh(mesh::cylinder(0.38f, 0.95f, 20));
    m_propMeshes[static_cast<size_t>(PropKind::Sphere)] = makeMesh(mesh::sphere(0.33f, 24, 16));
    m_propMeshes[static_cast<size_t>(PropKind::Plank)] = makeMesh(mesh::box({1.1f, 0.07f, 0.3f}));
    m_propMeshes[static_cast<size_t>(PropKind::Brick)] = makeMesh(mesh::box({0.25f, 0.12f, 0.12f}));
    m_propMeshes[static_cast<size_t>(PropKind::Can)] = makeMesh(mesh::cylinder(0.16f, 0.42f, 14));
    m_propMeshes[static_cast<size_t>(PropKind::Domino)] = makeMesh(mesh::box({0.2f, 0.35f, 0.05f}));
    m_propMeshes[static_cast<size_t>(PropKind::Cone)] = makeMesh(mesh::cone(0.25f, 0.6f, 16));
    m_propMeshes[static_cast<size_t>(PropKind::Pallet)] = makeMesh(mesh::box({0.6f, 0.07f, 0.5f}));
    m_propMeshes[static_cast<size_t>(PropKind::BigPlank)] = makeMesh(mesh::box({2.1f, 0.08f, 0.35f}));
    if (!m_beamMesh) return false;

    // First-person manipulator. The GLB is authored front-toward +Z, so it is
    // rotated 180 degrees to point down the camera's -Z forward axis.
    const gltf::Model manipulator =
        gltf::loadGLB(Paths::asset("models/void-manipulator.glb").string(), 2048);
    if (manipulator.valid()) {
        m_manipulatorMesh = makeMesh(manipulator.mesh);
        if (!manipulator.baseColor.pixels.empty()) {
            if (!m_device.createViewmodelTexture(manipulator.baseColor)) {
                log::warn("Manipulator texture upload failed");
            }
        }
        ViewPart part;
        part.mesh = m_manipulatorMesh;
        part.local = glm::translate(glm::mat4(1.0f), {0.26f, -0.21f, -1.04f}) *
                     glm::rotate(glm::mat4(1.0f), kPi + 0.14f, glm::vec3(0.0f, 1.0f, 0.0f)) *
                     glm::scale(glm::mat4(1.0f), glm::vec3(0.55f));
        part.color = {1.0f, 1.0f, 1.0f, 1.0f};
        part.roughness = 1.0f;
        part.metallic = 0.0f;
        part.emissive = 0.0f;
        part.unlit = true;
        part.viewTexture = true;
        m_viewParts.push_back(part);
    } else {
        log::warn("Manipulator model missing; using a simple placeholder");
        const rhi::MeshHandle body = makeMesh(mesh::box({0.05f, 0.06f, 0.22f}));
        const rhi::MeshHandle barrel = makeMesh(mesh::cylinder(0.028f, 0.26f, 12));
        ViewPart gripPart;
        gripPart.mesh = body;
        gripPart.local = trs({0.30f, -0.26f, -1.02f}, glm::quat{1, 0, 0, 0});
        gripPart.color = {0.13f, 0.15f, 0.18f, 1.0f};
        gripPart.roughness = 0.4f;
        gripPart.metallic = 0.8f;
        m_viewParts.push_back(gripPart);
        ViewPart barrelPart;
        barrelPart.mesh = barrel;
        barrelPart.local = trs({0.30f, -0.24f, -1.24f},
                               glm::angleAxis(kPi * 0.5f, glm::vec3(1, 0, 0)));
        barrelPart.color = {0.42f, 0.40f, 0.35f, 1.0f};
        barrelPart.roughness = 0.3f;
        barrelPart.metallic = 0.8f;
        m_viewParts.push_back(barrelPart);
    }

    if (!m_physics.initialize()) return false;
    m_player = m_physics.createPlayer(m_playerSpawn);
    if (!m_player.valid()) {
        log::error("Player body creation failed");
        return false;
    }

    buildLevel();
    buildProps();
    m_physics.settle(110);
    for (auto& prop : m_props) {
        prop->spawnPosition = m_physics.getPosition(prop->body);
        prop->spawnRotation = m_physics.getRotation(prop->body);
    }
    m_initialPropCount = static_cast<u32>(m_props.size());
    m_playerSpawn = m_physics.getPosition(m_player);
    syncProps();

    m_camera.position = m_playerSpawn + glm::vec3(0.0f, 0.8f, 0.0f);
    m_camera.yaw = 0.0f;
    m_camera.pitch = -0.03f;
    m_camera.fovDegrees = 78.0f;

    m_menuCamera.position = {7.6f, 3.9f, 11.2f};
    const glm::vec3 target{0.0f, 1.3f, -2.0f};
    const glm::vec3 dir = glm::normalize(target - m_menuCamera.position);
    m_menuCamera.yaw = std::atan2(-dir.x, -dir.z);
    m_menuCamera.pitch = std::asin(dir.y);
    m_menuCamera.fovDegrees = 68.0f;

    m_audio.init();
    m_backend = rhi::apiName(m_device.api());
    m_window.setRelativeMouse(false);
    m_window.showCursor(true);
    m_toast = "V.O.I.D. // BOX3D PHYSICS READY";
    m_toastTimer = 3.0f;
    m_ready = true;
    log::info("Demo ready: %u props, player tag %u", m_initialPropCount, kPlayerTag);
    return true;
}

void Game::shutdown() {
    if (m_held) releaseHeld(false);
    m_audio.shutdown();
    m_physics.shutdown();
}

void Game::addStaticBox(glm::vec3 size, glm::vec3 position, glm::quat rotation, u32 layer,
                        glm::vec2 uvScale, bool collider, bool shadow, glm::vec4 color,
                        float roughness, float metallic) {
    rhi::MeshHandle mesh = makeMesh(mesh::box(size * 0.5f));
    StaticDraw draw;
    draw.mesh = mesh;
    draw.transform = trs(position, rotation);
    draw.layer = layer;
    draw.uvScale = uvScale;
    draw.color = color;
    draw.roughness = roughness;
    draw.metallic = metallic;
    draw.castShadow = shadow;
    m_staticDraws.push_back(draw);
    if (collider) {
        m_physics.createBox(false, size * 0.5f, position, rotation, 0.0f, 0.75f, 0.04f, 0,
                            physics::Category::Static, false);
    }
}

void Game::addStaticVisual(glm::vec3 size, glm::vec3 position, glm::quat rotation, u32 layer,
                           glm::vec2 uvScale, glm::vec4 color) {
    addStaticBox(size, position, rotation, layer, uvScale, false, false, color);
}

void Game::buildLevel() {
    // Concrete floor with a separate visual plane, plus a thick collider.
    rhi::MeshHandle floorMesh = makeMesh(mesh::plane(30.0f, 28.0f));
    StaticDraw floorDraw;
    floorDraw.mesh = floorMesh;
    floorDraw.transform = glm::translate(glm::mat4(1.0f), {0, 0.001f, 0});
    floorDraw.layer = textures::Concrete;
    floorDraw.uvScale = {8, 8};
    floorDraw.color = {0.8f, 0.82f, 0.78f, 1};
    floorDraw.roughness = 0.94f;
    floorDraw.castShadow = false;
    m_staticDraws.push_back(floorDraw);
    m_physics.createBox(false, {15.0f, 0.25f, 14.0f}, {0, -0.25f, 0}, glm::quat{1,0,0,0},
                        0.0f, 0.8f, 0.05f, 0, physics::Category::Static, false);

    addStaticBox({30.6f, 10.6f, 0.6f}, {0, 5.3f, -14.0f}, glm::quat{1,0,0,0},
                 textures::Concrete, {5, 2});
    addStaticBox({30.6f, 10.6f, 0.6f}, {0, 5.3f, 14.0f}, glm::quat{1,0,0,0},
                 textures::Concrete, {5, 2});
    addStaticBox({0.6f, 10.6f, 28.6f}, {-15.0f, 5.3f, 0}, glm::quat{1,0,0,0},
                 textures::Concrete, {5, 2});
    addStaticBox({0.6f, 10.6f, 28.6f}, {15.0f, 5.3f, 0}, glm::quat{1,0,0,0},
                 textures::Concrete, {5, 2});

    // Industrial upper wall panels are visual only; the wall collider already covers them.
    addStaticVisual({30.6f, 4.8f, 0.62f}, {0, 7.9f, -13.99f}, glm::quat{1,0,0,0},
                    textures::Metal, {6, 1.8});
    addStaticVisual({30.6f, 4.8f, 0.62f}, {0, 7.9f, 13.99f}, glm::quat{1,0,0,0},
                    textures::Metal, {6, 1.8});
    addStaticVisual({0.62f, 4.8f, 28.6f}, {-14.99f, 7.9f, 0}, glm::quat{1,0,0,0},
                    textures::Metal, {6, 1.8});
    addStaticVisual({0.62f, 4.8f, 28.6f}, {14.99f, 7.9f, 0}, glm::quat{1,0,0,0},
                    textures::Metal, {6, 1.8});

    // Roof trusses and lamps.
    for (float z : {-8.5f, 0.0f, 8.5f}) {
        addStaticVisual({29.6f, 0.36f, 0.3f}, {0, 8.85f, z}, glm::quat{1,0,0,0},
                        textures::Metal, {1, 1}, {0.45f, 0.47f, 0.5f, 1});
    }

    // North-wall sign.
    rhi::MeshHandle signMesh = makeMesh(mesh::plane(7.6f, 3.1f));
    StaticDraw sign;
    sign.mesh = signMesh;
    sign.transform = trs({0, 5.85f, -13.67f}, glm::angleAxis(kPi * 0.5f, glm::vec3(1,0,0)));
    sign.layer = textures::Sign;
    sign.uvScale = {1, 1};
    sign.emissive = 0.25f;
    sign.castShadow = false;
    m_staticDraws.push_back(sign);
    addStaticVisual({7.9f, 3.4f, 0.08f}, {0, 5.85f, -13.73f}, glm::quat{1,0,0,0},
                    textures::Metal, {1, 1}, {0.2f, 0.22f, 0.22f, 1});

    // East platform.
    addStaticBox({6.2f, 0.3f, 6.0f}, {11.3f, 2.2f, -4.4f}, glm::quat{1,0,0,0},
                 textures::Concrete, {3, 3});
    addStaticBox({6.2f, 0.24f, 0.12f}, {11.3f, 2.2f, -1.42f}, glm::quat{1,0,0,0},
                 textures::Hazard, {4, 1});
    for (float x : {8.65f, 13.95f}) {
        for (float z : {-7.05f, -1.75f}) {
            addStaticBox({0.42f, 2.1f, 0.42f}, {x, 1.05f, z}, glm::quat{1,0,0,0},
                         textures::Metal, {1, 2});
        }
    }
    addStaticBox({6.1f, 0.07f, 0.07f}, {11.3f, 3.35f, -7.36f}, glm::quat{1,0,0,0},
                 textures::Metal, {1, 1});
    addStaticBox({0.07f, 0.07f, 5.9f}, {8.24f, 3.35f, -4.4f}, glm::quat{1,0,0,0},
                 textures::Metal, {1, 1});
    addStaticBox({1.45f, 0.07f, 0.07f}, {8.98f, 3.35f, -1.44f}, glm::quat{1,0,0,0},
                 textures::Metal, {1, 1});
    addStaticBox({1.45f, 0.07f, 0.07f}, {13.62f, 3.35f, -1.44f}, glm::quat{1,0,0,0},
                 textures::Metal, {1, 1});

    // Ramp.
    const float rampAngle = std::atan2(2.35f, 6.0f);
    addStaticBox({3.0f, 0.3f, 6.5f}, {11.3f, 1.16f, 1.65f},
                 glm::angleAxis(rampAngle, glm::vec3(1,0,0)), textures::Concrete, {3, 3});
    for (float x : {9.82f, 12.78f}) {
        addStaticVisual({0.14f, 0.07f, 6.5f}, {x, 1.34f, 1.65f},
                        glm::angleAxis(rampAngle, glm::vec3(1,0,0)), textures::Hazard, {4, 1});
    }

    // West shelf.
    for (float x : {-13.65f, -12.65f}) {
        for (float z : {-8.55f, -4.45f}) {
            addStaticBox({0.08f, 3.35f, 0.08f}, {x, 1.675f, z}, glm::quat{1,0,0,0},
                         textures::Metal, {1, 1});
        }
    }
    for (float y : {0.9f, 1.95f, 3.0f}) {
        addStaticBox({1.05f, 0.08f, 4.2f}, {-13.15f, y, -6.5f}, glm::quat{1,0,0,0},
                     textures::Metal, {2, 3});
    }

    // Seesaw fulcrum.
    addStaticBox({0.62f, 0.84f, 0.9f}, {-7.8f, 0.42f, -2.6f}, glm::quat{1,0,0,0},
                 textures::Metal, {1, 1});
    addStaticVisual({0.68f, 0.14f, 0.96f}, {-7.8f, 0.6f, -2.6f}, glm::quat{1,0,0,0},
                    textures::Hazard, {2, 1});

    // Painted floor lines.
    addStaticVisual({0.12f, 0.012f, 7.0f}, {-1.6f, 0.012f, 6.8f}, glm::quat{1,0,0,0},
                    textures::White, {1, 1}, {0.55f, 0.38f, 0.08f, 1});
    addStaticVisual({0.12f, 0.012f, 7.0f}, {1.6f, 0.012f, 6.8f}, glm::quat{1,0,0,0},
                    textures::White, {1, 1}, {0.55f, 0.38f, 0.08f, 1});
}

void Game::buildProps() {
    auto create = [&](PropKind kind, glm::vec3 position, glm::quat rotation = glm::quat{1,0,0,0},
                      bool initial = true) {
        static const PropKindInfo info[] = {
            {"Ящик", textures::Crate, {1,1,1,1}, 0.82f, 0.02f, 0.55f, 0.08f, 17.8f, 0.45f, 0.45f, 0.45f},
            {"Бочка", textures::Barrel, {0.72f,0.9f,1,1}, 0.46f, 0.35f, 0.42f, 0.2f, 46.4f, 0.38f, 0.475f, 0.38f},
            {"Стальной шар", textures::Metal, {0.85f,0.9f,0.92f,1}, 0.18f, 0.95f, 0.28f, 0.42f, 73.0f, 0.33f, 0.33f, 0.33f},
            {"Доска", textures::Wood, {1,1,1,1}, 0.72f, 0.02f, 0.55f, 0.1f, 35.0f, 1.1f, 0.07f, 0.3f},
            {"Кирпич", textures::Concrete, {0.72f,0.34f,0.26f,1}, 0.92f, 0.0f, 0.72f, 0.04f, 111.0f, 0.25f, 0.12f, 0.12f},
            {"Канистра", textures::Metal, {0.9f,0.45f,0.28f,1}, 0.3f, 0.6f, 0.4f, 0.25f, 41.0f, 0.16f, 0.21f, 0.16f},
            {"Домино", textures::White, {0.92f,0.88f,0.76f,1}, 0.55f, 0.02f, 0.48f, 0.12f, 42.8f, 0.2f, 0.35f, 0.05f},
            {"Конус", textures::Hazard, {1,0.42f,0.08f,1}, 0.6f, 0.05f, 0.6f, 0.2f, 56.0f, 0.25f, 0.3f, 0.25f},
            {"Паллета", textures::Wood, {0.8f,0.68f,0.48f,1}, 0.88f, 0.02f, 0.6f, 0.1f, 29.8f, 0.6f, 0.07f, 0.5f},
            {"Балансир", textures::Wood, {1,1,1,1}, 0.76f, 0.02f, 0.55f, 0.1f, 25.5f, 2.1f, 0.08f, 0.35f},
        };
        const PropKindInfo& ki = info[static_cast<size_t>(kind)];
        auto prop = std::make_unique<Prop>();
        prop->kind = kind;
        prop->tag = m_nextTag++;
        prop->mesh = m_propMeshes[static_cast<size_t>(kind)];
        prop->initial = initial;

        if (kind == PropKind::Sphere) {
            prop->body = m_physics.createSphere(true, ki.halfX, position, ki.density,
                                               ki.friction, ki.restitution, prop->tag);
        } else if (kind == PropKind::Barrel || kind == PropKind::Can) {
            prop->body = m_physics.createCylinderHull(true, ki.halfX, kind == PropKind::Can ? 0.42f : 0.95f,
                                                      position, rotation, ki.density, ki.friction,
                                                      ki.restitution, prop->tag);
        } else if (kind == PropKind::Cone) {
            prop->body = m_physics.createConeHull(true, ki.halfX, 0.6f, position, rotation,
                                                  ki.density, ki.friction, ki.restitution,
                                                  prop->tag);
        } else {
            prop->body = m_physics.createBox(true, {ki.halfX, ki.halfY, ki.halfZ}, position, rotation,
                                             ki.density, ki.friction, ki.restitution, prop->tag);
        }
        if (!prop->body.valid()) return;

        prop->draw.mesh = prop->mesh;
        prop->draw.transform = trs(position, rotation);
        prop->draw.textureLayer = ki.layer;
        prop->draw.baseColor = ki.color;
        prop->draw.roughness = ki.roughness;
        prop->draw.metallic = ki.metallic;
        prop->draw.uvScale = {1, 1};
        prop->draw.castShadow = true;
        if (kind == PropKind::Barrel && (m_props.size() % 3) == 1) prop->draw.baseColor = {1.0f, 0.65f, 0.55f, 1};
        if (kind == PropKind::Barrel && (m_props.size() % 3) == 2) prop->draw.baseColor = {0.65f, 1.0f, 0.65f, 1};
        prop->spawnPosition = position;
        prop->spawnRotation = rotation;
        m_propsByTag[prop->tag] = prop.get();
        m_props.push_back(std::move(prop));
    };

    const float baseZ = -7.7f;
    create(PropKind::Crate, {-0.57f, 0.47f, baseZ});
    create(PropKind::Crate, {0.35f, 0.47f, baseZ});
    create(PropKind::Crate, {1.27f, 0.47f, baseZ});
    create(PropKind::Crate, {-0.11f, 1.38f, baseZ});
    create(PropKind::Crate, {0.81f, 1.38f, baseZ});
    create(PropKind::Crate, {0.35f, 2.29f, baseZ});

    create(PropKind::Crate, {3.35f, 0.47f, 3.5f});
    create(PropKind::Crate, {4.28f, 0.47f, 3.85f}, glm::angleAxis(0.32f, glm::vec3(0,1,0)));
    create(PropKind::Crate, {3.7f, 0.47f, 4.65f});
    create(PropKind::Crate, {5.45f, 0.47f, 2.9f});
    create(PropKind::Crate, {5.43f, 1.38f, 2.88f});

    create(PropKind::Barrel, {-4.2f, 0.49f, 3.35f});
    create(PropKind::Barrel, {-5.15f, 0.49f, 3.1f});
    create(PropKind::Barrel, {-4.8f, 0.49f, 4.2f});
    create(PropKind::Barrel, {-5.95f, 0.49f, 3.95f});
    create(PropKind::Barrel, {-6.6f, 0.49f, 4.65f});

    create(PropKind::Sphere, {1.2f, 0.34f, 4.85f});
    create(PropKind::Sphere, {2.05f, 0.34f, 4.35f});
    create(PropKind::Sphere, {0.25f, 0.34f, 5.35f});

    create(PropKind::Plank, {2.75f, 0.08f, 2.0f}, glm::angleAxis(0.5f, glm::vec3(0,1,0)));
    create(PropKind::Plank, {-9.6f, 0.08f, 2.6f}, glm::angleAxis(-0.25f, glm::vec3(0,1,0)));

    for (int i = 0; i < 9; ++i) {
        create(PropKind::Domino, {7.25f, 0.36f, 2.35f + i * 0.47f});
    }

    create(PropKind::Brick, {-9.6f, 0.13f, -0.9f});
    create(PropKind::Brick, {-9.05f, 0.13f, -0.75f});
    create(PropKind::Brick, {-9.35f, 0.37f, -0.82f});
    create(PropKind::Brick, {-10.2f, 0.13f, -1.4f});

    create(PropKind::Can, {2.35f, 0.22f, 1.55f});
    create(PropKind::Can, {2.55f, 0.22f, 1.85f});
    create(PropKind::Cone, {9.45f, 0.31f, 4.9f});
    create(PropKind::Cone, {8.65f, 0.31f, 4.25f});
    create(PropKind::Pallet, {5.75f, 0.08f, 1.25f});
    create(PropKind::Pallet, {-2.4f, 0.08f, -0.9f});
    create(PropKind::BigPlank, {-7.8f, 0.93f, -2.6f});

    create(PropKind::Crate, {-13.15f, 1.4f, -5.55f});
    create(PropKind::Brick, {-13.15f, 2.11f, -6.4f});
    create(PropKind::Brick, {-13.15f, 2.11f, -6.05f});
    create(PropKind::Sphere, {-13.15f, 3.38f, -7.2f});

    create(PropKind::Crate, {10.15f, 2.82f, -3.15f});
    create(PropKind::Crate, {12.4f, 2.82f, -5.55f});
    create(PropKind::Barrel, {13.1f, 2.84f, -2.65f});
    create(PropKind::Sphere, {9.45f, 2.7f, -5.9f});
    create(PropKind::Plank, {11.8f, 2.5f, -2.6f}, glm::angleAxis(0.9f, glm::vec3(0,1,0)));
}

void Game::syncProps() {
    for (auto& prop : m_props) {
        prop->draw.transform = trs(m_physics.getPosition(prop->body),
                                   m_physics.getRotation(prop->body));
    }
}

Prop* Game::findProp(u32 tag) {
    auto it = m_propsByTag.find(tag);
    return it == m_propsByTag.end() ? nullptr : it->second;
}

void Game::startGame() {
    m_state = State::Playing;
    m_keepHeld = false;
    log::info("Game started");
    m_window.setRelativeMouse(true);
    m_window.showCursor(false);
    m_camera.position = m_playerSpawn + glm::vec3(0.0f, 0.8f, 0.0f);
    m_camera.yaw = 0.0f;
    m_camera.pitch = -0.03f;
    m_camera.fovDegrees = 78.0f;
    m_toast = "ЛАБОРАТОРИЯ АКТИВНА // ЛКМ — СХВАТИТЬ";
    m_toastTimer = 3.0f;
}

void Game::pauseGame() {
    if (m_state != State::Playing) return;
    m_state = State::Paused;
    log::info("Paused");
    m_window.setRelativeMouse(false);
    m_window.showCursor(true);
    if (m_held) releaseHeld(false);
}

void Game::resumeGame() {
    if (m_state != State::Paused) return;
    m_state = State::Playing;
    log::info("Resumed");
    m_window.setRelativeMouse(true);
    m_window.showCursor(false);
}

void Game::updatePlayer(float dt) {
    if (!m_player.valid()) return;
    const glm::vec3 bodyPos = m_physics.getPosition(m_player);
    const physics::RayHit ground = m_physics.raycast(bodyPos, {0, -1, 0}, 0.95f);
    const bool grounded = ground.hit && ground.distance <= 0.86f;

    glm::vec3 forward{-std::sin(m_camera.yaw), 0.0f, -std::cos(m_camera.yaw)};
    glm::vec3 right{std::cos(m_camera.yaw), 0.0f, -std::sin(m_camera.yaw)};
    glm::vec3 wish{0.0f};
    if (SDL_GetKeyboardState(nullptr)[SDL_SCANCODE_W]) wish += forward;
    if (SDL_GetKeyboardState(nullptr)[SDL_SCANCODE_S]) wish -= forward;
    if (SDL_GetKeyboardState(nullptr)[SDL_SCANCODE_D]) wish += right;
    if (SDL_GetKeyboardState(nullptr)[SDL_SCANCODE_A]) wish -= right;
    if (glm::length(wish) > 1e-4f) wish = glm::normalize(wish);

    const bool sprint = SDL_GetKeyboardState(nullptr)[SDL_SCANCODE_LSHIFT];
    const float targetSpeed = sprint ? 7.0f : 4.35f;
    const glm::vec3 velocity = m_physics.getLinearVelocity(m_player);
    const float blend = 1.0f - std::exp(-(grounded ? 14.0f : 3.0f) * dt);
    glm::vec3 wanted = wish * targetSpeed;
    m_physics.setLinearVelocity(m_player,
        {velocity.x + (wanted.x - velocity.x) * blend, velocity.y,
         velocity.z + (wanted.z - velocity.z) * blend});

    if (grounded && SDL_GetKeyboardState(nullptr)[SDL_SCANCODE_SPACE]) {
        m_physics.setLinearVelocity(m_player, {wanted.x, 5.5f, wanted.z});
    }

    const float speed = glm::length(glm::vec2(velocity.x, velocity.z));
    if (grounded && speed > 0.8f) {
        m_footstepTimer -= dt;
        if (m_footstepTimer <= 0.0f) {
            m_audio.footstep(speed);
            m_footstepTimer = glm::clamp(0.62f - speed * 0.04f, 0.32f, 0.55f);
        }
    } else if (!grounded) {
        m_footstepTimer = 0.12f;
    }
    const float bob = grounded ? std::sin(m_time * (6.0f + speed)) * 0.018f * std::min(1.0f, speed / 5.0f) : 0.0f;
    m_camera.position = bodyPos + glm::vec3(0.0f, 0.8f + bob, 0.0f);
    m_camera.fovDegrees = sprint && speed > 5.0f ? 81.0f : 78.0f;
}

void Game::updateHover() {
    if (m_state != State::Playing) {
        m_hover = nullptr;
        return;
    }
    const physics::RayHit hit = m_physics.raycast(m_camera.position, m_camera.forward(), 6.0f);
    m_hover = hit.hit ? findProp(hit.body.tag) : nullptr;
}

void Game::tryGrab() {
    if (!m_hover || m_held) return;
    m_held = m_hover;
    m_audio.zap();
    m_heldSavedFilter = m_physics.getFilter(m_held->body);
    physics::ShapeFilter filter = m_heldSavedFilter;
    filter.maskBits = static_cast<u64>(physics::Category::Static) |
                      static_cast<u64>(physics::Category::Prop);
    m_physics.setFilter(m_held->body, filter);
    m_holdDistance = glm::clamp(
        glm::length(m_physics.getPosition(m_held->body) - m_camera.position), 1.3f, 5.5f);
    m_toast = "ЗАХВАТ: " + std::string(m_held->kind == PropKind::Crate ? "ЯЩИК" : "ОБЪЕКТ");
    m_toastTimer = 1.0f;
}

void Game::releaseHeld(bool punt) {
    if (!m_held) return;
    Prop* prop = m_held;
    m_physics.setFilter(prop->body, m_heldSavedFilter);
    if (punt) {
        m_audio.throwSound();
        const float mass = m_physics.getMass(prop->body);
        m_physics.applyImpulse(prop->body, m_camera.forward() * mass * 12.0f +
                               glm::vec3(0, mass * 2.2f, 0));
    }
    m_held = nullptr;
}

void Game::updateGrab(float dt) {
    (void)dt;
    if (!m_held) return;
    const glm::vec3 target = m_camera.position + m_camera.forward() * m_holdDistance;
    const glm::vec3 position = m_physics.getPosition(m_held->body);
    const glm::vec3 velocity = m_physics.getLinearVelocity(m_held->body);
    glm::vec3 delta = target - position;
    if (glm::length(delta) > 3.4f) {
        releaseHeld(false);
        return;
    }
    const float mass = std::max(0.5f, m_physics.getMass(m_held->body));
    glm::vec3 force = delta * (90.0f * mass) - velocity * (19.0f * mass);
    const float maxForce = mass * 300.0f;
    if (glm::length(force) > maxForce) force = glm::normalize(force) * maxForce;
    m_physics.applyForce(m_held->body, force);
}

void Game::shove() {
    const physics::RayHit hit = m_physics.raycast(m_camera.position, m_camera.forward(), 3.8f);
    if (!hit.hit) return;
    Prop* prop = findProp(hit.body.tag);
    if (!prop) return;
    m_audio.throwSound();
    const float mass = m_physics.getMass(prop->body);
    m_physics.applyImpulseAt(prop->body, m_camera.forward() * mass * 6.5f +
                             glm::vec3(0, mass * 1.0f, 0), hit.point);
}

void Game::spawnProp(PropKind kind) {
    if (m_state != State::Playing) return;
    const physics::RayHit hit = m_physics.raycast(m_camera.position, m_camera.forward(), 5.0f);
    glm::vec3 position;
    if (hit.hit) position = hit.point + hit.normal * 0.55f;
    else position = m_camera.position + m_camera.forward() * 2.5f;
    position = glm::clamp(position, glm::vec3(-13.8f, 0.6f, -13.0f), glm::vec3(13.8f, 8.0f, 13.0f));

    // buildProps() uses a local lambda, so runtime spawning shares the same
    // material table but creates the body here.
    static const PropKindInfo info[] = {
        {"Ящик", textures::Crate, {1,1,1,1}, 0.82f, 0.02f, 0.55f, 0.08f, 17.8f, 0.45f, 0.45f, 0.45f},
        {"Бочка", textures::Barrel, {0.72f,0.9f,1,1}, 0.46f, 0.35f, 0.42f, 0.2f, 46.4f, 0.38f, 0.475f, 0.38f},
        {"Стальной шар", textures::Metal, {0.85f,0.9f,0.92f,1}, 0.18f, 0.95f, 0.28f, 0.42f, 73.0f, 0.33f, 0.33f, 0.33f},
        {"Доска", textures::Wood, {1,1,1,1}, 0.72f, 0.02f, 0.55f, 0.1f, 35.0f, 1.1f, 0.07f, 0.3f},
        {"Кирпич", textures::Concrete, {0.72f,0.34f,0.26f,1}, 0.92f, 0.0f, 0.72f, 0.04f, 111.0f, 0.25f, 0.12f, 0.12f},
        {"Канистра", textures::Metal, {0.9f,0.45f,0.28f,1}, 0.3f, 0.6f, 0.4f, 0.25f, 41.0f, 0.16f, 0.21f, 0.16f},
        {"Домино", textures::White, {0.92f,0.88f,0.76f,1}, 0.55f, 0.02f, 0.48f, 0.12f, 42.8f, 0.2f, 0.35f, 0.05f},
        {"Конус", textures::Hazard, {1,0.42f,0.08f,1}, 0.6f, 0.05f, 0.6f, 0.2f, 56.0f, 0.25f, 0.3f, 0.25f},
        {"Паллета", textures::Wood, {0.8f,0.68f,0.48f,1}, 0.88f, 0.02f, 0.6f, 0.1f, 29.8f, 0.6f, 0.07f, 0.5f},
        {"Балансир", textures::Wood, {1,1,1,1}, 0.76f, 0.02f, 0.55f, 0.1f, 25.5f, 2.1f, 0.08f, 0.35f},
    };
    const PropKindInfo& ki = info[static_cast<size_t>(kind)];
    auto prop = std::make_unique<Prop>();
    prop->kind = kind;
    prop->tag = m_nextTag++;
    prop->mesh = m_propMeshes[static_cast<size_t>(kind)];
    prop->initial = false;

    if (kind == PropKind::Sphere) {
        prop->body = m_physics.createSphere(true, ki.halfX, position, ki.density, ki.friction,
                                            ki.restitution, prop->tag);
    } else if (kind == PropKind::Barrel || kind == PropKind::Can) {
        prop->body = m_physics.createCylinderHull(true, ki.halfX,
            kind == PropKind::Can ? 0.42f : 0.95f, position, glm::quat{1,0,0,0}, ki.density,
            ki.friction, ki.restitution, prop->tag);
    } else if (kind == PropKind::Cone) {
        prop->body = m_physics.createConeHull(true, ki.halfX, 0.6f, position,
            glm::quat{1,0,0,0}, ki.density, ki.friction, ki.restitution, prop->tag);
    } else {
        prop->body = m_physics.createBox(true, {ki.halfX, ki.halfY, ki.halfZ}, position,
            glm::quat{1,0,0,0}, ki.density, ki.friction, ki.restitution, prop->tag);
    }
    if (!prop->body.valid()) return;
    prop->draw.mesh = prop->mesh;
    prop->draw.transform = trs(position, glm::quat{1,0,0,0});
    prop->draw.textureLayer = ki.layer;
    prop->draw.baseColor = ki.color;
    prop->draw.roughness = ki.roughness;
    prop->draw.metallic = ki.metallic;
    prop->draw.uvScale = {1, 1};
    prop->draw.castShadow = true;
    prop->spawnPosition = position;
    prop->spawnRotation = glm::quat{1,0,0,0};
    m_propsByTag[prop->tag] = prop.get();
    m_props.push_back(std::move(prop));
    m_audio.spawn();

    u32 spawned = 0;
    for (auto& p : m_props) if (!p->initial) ++spawned;
    while (spawned > kSpawnLimit) {
        auto it = std::find_if(m_props.begin(), m_props.end(), [](const auto& p) { return !p->initial; });
        if (it == m_props.end()) break;
        removeProp(**it);
        --spawned;
    }
}

void Game::spawnRandom() {
    const PropKind kinds[] = {PropKind::Crate, PropKind::Barrel, PropKind::Sphere, PropKind::Plank,
                              PropKind::Brick, PropKind::Can, PropKind::Domino, PropKind::Cone,
                              PropKind::Pallet};
    spawnProp(kinds[std::rand() % (sizeof(kinds) / sizeof(kinds[0]))]);
}

void Game::removeProp(Prop& prop) {
    if (&prop == m_held) {
        m_held = nullptr;
    }
    m_propsByTag.erase(prop.tag);
    m_physics.destroyBody(prop.body);
    m_props.erase(std::remove_if(m_props.begin(), m_props.end(),
                                 [&](const auto& p) { return p.get() == &prop; }),
                  m_props.end());
}

void Game::resetScene() {
    if (m_held) releaseHeld(false);
    for (auto it = m_props.begin(); it != m_props.end();) {
        if (!(*it)->initial) {
            m_propsByTag.erase((*it)->tag);
            m_physics.destroyBody((*it)->body);
            it = m_props.erase(it);
        } else {
            ++it;
        }
    }
    for (auto& prop : m_props) {
        m_physics.setTransform(prop->body, prop->spawnPosition, prop->spawnRotation);
        m_physics.setLinearVelocity(prop->body, {0, 0, 0});
        m_physics.setAngularVelocity(prop->body, {0, 0, 0});
        m_physics.setAwake(prop->body, true);
        prop->draw.transform = trs(prop->spawnPosition, prop->spawnRotation);
    }
    m_physics.setTransform(m_player, m_playerSpawn, glm::quat{1,0,0,0});
    m_physics.setLinearVelocity(m_player, {0, 0, 0});
    m_camera.position = m_playerSpawn + glm::vec3(0, 0.8f, 0);
    m_camera.yaw = 0;
    m_camera.pitch = -0.03f;
    m_toast = "СЦЕНА ВОССТАНОВЛЕНА";
    m_toastTimer = 1.6f;
}

void Game::addToast(const std::string& text) {
    m_toast = text;
    m_toastTimer = 1.8f;
}

void Game::onHit(u32 tagA, u32 tagB, float speed) {
    if (m_state != State::Playing) return;
    Prop* prop = findProp(tagA);
    if (!prop) prop = findProp(tagB);
    if (!prop) return;
    const char* material = "wood";
    switch (prop->kind) {
    case PropKind::Barrel:
    case PropKind::Sphere:
    case PropKind::Can: material = "metal"; break;
    case PropKind::Brick: material = "concrete"; break;
    case PropKind::Cone: material = "plastic"; break;
    default: material = "wood"; break;
    }
    m_audio.impact(material, speed / 9.0f);
}

void Game::buildViewmodel(rhi::DrawList& draws) {
    if (m_viewParts.empty()) return;
    const glm::mat4 camWorld = glm::inverse(m_camera.view());
    for (const auto& part : m_viewParts) {
        rhi::DrawItem item;
        item.mesh = part.mesh;
        item.transform = camWorld * part.local;
        item.textureLayer = part.viewTexture ? 255u : textures::White;
        item.baseColor = part.color;
        item.roughness = part.roughness;
        item.metallic = part.metallic;
        item.emissive = part.emissive;
        item.uvScale = {1, 1};
        item.castShadow = false;
        item.viewmodel = true;
        item.unlit = part.unlit;
        draws.items.push_back(item);
    }
}

void Game::buildBeam(rhi::DrawList& draws) {
    if (!m_held) return;
    const glm::mat4 camWorld = glm::inverse(m_camera.view());
    const glm::vec3 start = glm::vec3(camWorld * glm::vec4(0.222f, -0.195f, -1.33f, 1.0f));
    glm::vec3 end = m_physics.getPosition(m_held->body);
    const glm::vec3 toStart = glm::normalize(start - end);
    end += toStart * 0.28f;
    const glm::vec3 segment = end - start;
    const float length = glm::length(segment);
    if (length < 0.05f) return;
    const glm::vec3 dir = segment / length;
    glm::vec3 up{0, 1, 0};
    if (std::abs(glm::dot(dir, up)) > 0.98f) up = {0, 0, 1};
    const glm::vec3 x = glm::normalize(glm::cross(up, dir));
    const glm::vec3 y = glm::cross(dir, x);
    const glm::quat q = glm::quat_cast(glm::mat3(x, y, dir));
    rhi::DrawItem item;
    item.mesh = m_beamMesh;
    item.transform = trs((start + end) * 0.5f, q, {1, 1, length});
    item.textureLayer = textures::White;
    item.baseColor = {1.0f, 0.45f, 0.12f, 1.0f};
    item.emissive = 3.0f;
    item.roughness = 0.4f;
    item.castShadow = false;
    draws.items.push_back(item);
}

bool Game::selfTest() {
    startGame();
    m_physics.setTransform(m_player, {3.33f, 0.9f, 5.7f}, glm::quat{1,0,0,0});
    m_camera.position = {3.33f, 1.7f, 5.7f};
    m_camera.yaw = 0.0f;
    m_camera.pitch = -0.52f;
    updateHover();
    Prop* target = m_hover;
    if (!target) {
        log::error("SELFTEST: no target under crosshair");
        return false;
    }
    const glm::vec3 targetPos = m_physics.getPosition(target->body);
    const bool hoverOk = true;
    tryGrab();
    const bool grabbed = m_held == target;
    const glm::vec3 before = m_physics.getPosition(target->body);
    for (int i = 0; i < 90; ++i) {
        updateGrab(1.0f / 60.0f);
        m_physics.step(1.0f / 60.0f);
    }
    const glm::vec3 heldPos = m_physics.getPosition(target->body);
    const float moved = glm::length(heldPos - before);
    releaseHeld(true);
    const glm::vec3 thrownVel = m_physics.getLinearVelocity(target->body);
    const float thrownSpeed = glm::length(thrownVel);
    log::info("SELFTEST target=%s tag=%u mass=%.1f pos=(%.2f,%.2f,%.2f)",
              target->kind == PropKind::Crate ? "crate" : "prop", target->tag,
              m_physics.getMass(target->body), targetPos.x, targetPos.y, targetPos.z);
    resetScene();
    const bool resetOk = m_props.size() == m_initialPropCount;
    spawnProp(PropKind::Crate);
    const bool spawnOk = m_props.size() == m_initialPropCount + 1;
    pauseGame();
    const bool pauseOk = m_state == State::Paused;
    resumeGame();
    const bool resumeOk = m_state == State::Playing;
    const bool ok = hoverOk && grabbed && moved > 0.15f && thrownSpeed > 2.0f && resetOk &&
                    spawnOk && pauseOk && resumeOk;
    log::info("SELFTEST hover=%d grab=%d moved=%.2f throwSpeed=%.2f reset=%d spawn=%d pause=%d resume=%d => %s",
              hoverOk ? 1 : 0, grabbed ? 1 : 0, moved, thrownSpeed, resetOk ? 1 : 0,
              spawnOk ? 1 : 0, pauseOk ? 1 : 0, resumeOk ? 1 : 0, ok ? "PASS" : "FAIL");
    return ok;
}

void Game::showcase() {
    startGame();
    m_keepHeld = true;
    m_physics.setTransform(m_player, {3.33f, 0.9f, 7.5f}, glm::quat{1,0,0,0});
    m_camera.position = {3.33f, 1.7f, 7.5f};
    m_camera.yaw = 0.0f;
    m_camera.pitch = -0.32f;
    updateHover();
    tryGrab();
    m_toast.clear();
    m_toastTimer = 0.0f;
}

void Game::buildFrame(rhi::FrameData& frame, rhi::DrawList& draws, rhi::UIList& ui) {
    Camera cam = (m_state == State::Menu) ? m_menuCamera : m_camera;
    cam.aspect = static_cast<float>(m_window.width()) / std::max(1u, m_window.height());
    frame.viewProj = cam.viewProjection();
    frame.invViewProj = glm::inverse(frame.viewProj);
    frame.cameraPos = {cam.position, m_time};
    frame.sunDirection = glm::vec4(glm::normalize(m_sunDir), 0.0f);
    frame.sunColor = {1.0f, 0.9f, 0.74f, 2.25f};
    frame.ambient = {0.34f, 0.38f, 0.43f, 0.24f};
    frame.fog = {0.46f, 0.45f, 0.42f, 0.006f};
    frame.skyTop = {0.10f, 0.18f, 0.30f, 1.0f};
    frame.skyHorizon = {0.60f, 0.53f, 0.42f, 1.0f};
    frame.params = {0.9f, m_time, 0.0f, 0.0f};
    frame.pointPos[0] = {-7.2f, 7.4f, -5.5f, 17.0f};
    frame.pointPos[1] = {7.2f, 7.4f, -5.5f, 17.0f};
    frame.pointPos[2] = {0.0f, 7.4f, 4.5f, 17.0f};
    frame.pointColor[0] = {1.0f, 0.72f, 0.42f, 28.0f};
    frame.pointColor[1] = {1.0f, 0.72f, 0.42f, 28.0f};
    frame.pointColor[2] = {1.0f, 0.72f, 0.42f, 24.0f};

    glm::vec3 sunDir = glm::normalize(m_sunDir);
    glm::mat4 lightView = glm::lookAt(sunDir * 42.0f, glm::vec3(0), glm::vec3(0, 1, 0));
    glm::mat4 lightProj = glm::orthoRH_ZO(-21.0f, 21.0f, -21.0f, 21.0f, 0.1f, 100.0f);
    glm::mat4 bias(0.5f, 0.0f, 0.0f, 0.0f,
                   0.0f, -0.5f, 0.0f, 0.0f,
                   0.0f, 0.0f, 1.0f, 0.0f,
                   0.5f, 0.5f, 0.0f, 1.0f);
    draws.shadowViewProj = lightProj * lightView;
    frame.shadowMatrix = bias * draws.shadowViewProj;

    for (const auto& s : m_staticDraws) {
        rhi::DrawItem item;
        item.mesh = s.mesh;
        item.transform = s.transform;
        item.textureLayer = s.layer;
        item.baseColor = s.color;
        item.uvScale = s.uvScale;
        item.roughness = s.roughness;
        item.metallic = s.metallic;
        item.emissive = s.emissive;
        item.castShadow = s.castShadow;
        draws.items.push_back(item);
    }

    for (const auto& prop : m_props) {
        rhi::DrawItem item = prop->draw;
        item.transform = trs(m_physics.getPosition(prop->body), m_physics.getRotation(prop->body));
        if (prop.get() == m_held) item.emissive = std::max(item.emissive, 0.32f);
        draws.items.push_back(item);
    }

    buildBeam(draws);
    if (m_state != State::Menu) buildViewmodel(draws);

    if (m_state == State::Menu) buildMenuUI(ui);
    else if (m_state == State::Paused) {
        buildHudUI(ui);
        buildPauseUI(ui);
    } else {
        buildHudUI(ui);
    }
    ui.textureWidth = static_cast<u32>(m_font.atlasWidth());
    ui.textureHeight = static_cast<u32>(m_font.atlasHeight());
}

void Game::buildMenuUI(rhi::UIList& ui) {
    const float W = static_cast<float>(m_window.width());
    const float H = static_cast<float>(m_window.height());
    addRect(ui, m_font, 0, 0, 760, H, rgba(0.025f, 0.032f, 0.034f, 0.94f));
    addRect(ui, m_font, 760, 0, 4, H, rgba(0.95f, 0.58f, 0.18f, 0.8f));

    addText(ui, m_font, 64, 56, 12, rgba(1.0f, 0.7f, 0.28f), "V.O.I.D. // ENGINE");
    addText(ui, m_font, 64, 132, 84, rgba(1.0f, 1.0f, 1.0f), "PHYSICS");
    addText(ui, m_font, 64, 218, 84, rgba(1.0f, 0.62f, 0.18f), "LAB");
    addRect(ui, m_font, 64, 316, 420, 2, rgba(1.0f, 0.62f, 0.18f, 0.75f));
    addText(ui, m_font, 64, 338, 17, rgba(0.92f, 0.89f, 0.82f),
            "Склад с честной физикой Box3D. Хватайте,");
    addText(ui, m_font, 64, 362, 17, rgba(0.92f, 0.89f, 0.82f),
            "таскайте, толкайте и швыряйте всё, что не прикручено.");
    addText(ui, m_font, 64, 392, 11, rgba(0.85f, 0.82f, 0.74f, 0.5f),
            "NATIVE C++20 · BOX3D PHYSICS · DX12 / VULKAN");

    const float bx = 64, by = 442, bw = 390, bh = 54;
    addRect(ui, m_font, bx, by, bw, bh, rgba(0.95f, 0.55f, 0.16f, 0.16f));
    addBorder(ui, m_font, bx, by, bw, bh, 1.5f, rgba(1.0f, 0.66f, 0.25f, 0.8f));
    addText(ui, m_font, bx + 22, by + 17, 14, rgba(1.0f, 0.85f, 0.64f), "ВОЙТИ В ЛАБОРАТОРИЮ");
    addText(ui, m_font, bx + bw - 36, by + 12, 24, rgba(1.0f, 0.66f, 0.25f), "→");

    addText(ui, m_font, 64, 530, 10, rgba(1.0f, 0.7f, 0.28f, 0.8f), "УПРАВЛЕНИЕ");
    addText(ui, m_font, 64, 552, 13, rgba(0.9f, 0.88f, 0.82f, 0.9f), "WASD  движение     SHIFT  бег     SPACE  прыжок");
    addText(ui, m_font, 64, 574, 13, rgba(0.9f, 0.88f, 0.82f, 0.9f), "ЛКМ  схватить       ПКМ  толчок   F  швырнуть");
    addText(ui, m_font, 64, 596, 13, rgba(0.9f, 0.88f, 0.82f, 0.9f), "1–7  спавн          G  случайный  R  сброс");
    addText(ui, m_font, 64, 618, 13, rgba(0.9f, 0.88f, 0.82f, 0.9f), "Q  slow-mo          M  звук       ESC  пауза");

    addText(ui, m_font, 64, H - 66, 10, rgba(0.86f, 0.84f, 0.78f, 0.3f),
            "PHYSICS: BOX3D BY ERIN CATTO · WASM-FREE NATIVE BUILD");
    addText(ui, m_font, 64, H - 44, 9, rgba(0.86f, 0.84f, 0.78f, 0.25f),
            "V.O.I.D. ENGINE 0.1.0 · AUTHOR MI5HLEN (МИХАИЛ ОДНОЛЕТКО)");
    addText(ui, m_font, W - 240, H - 38, 9, rgba(0.86f, 0.84f, 0.78f, 0.35f), m_backend);
}

void Game::buildHudUI(rhi::UIList& ui) {
    const float W = static_cast<float>(m_window.width());
    const float H = static_cast<float>(m_window.height());
    const float cx = W * 0.5f, cy = H * 0.5f;
    const bool active = m_hover != nullptr || m_held != nullptr;
    const glm::vec4 accent = active ? rgba(1.0f, 0.62f, 0.18f) : rgba(0.95f, 0.92f, 0.85f, 0.75f);

    addRect(ui, m_font, cx - 1, cy - 1, 2, 2, accent);
    const float gap = active ? 10.0f : 13.0f;
    addRect(ui, m_font, cx, cy - gap, 2, 7, accent);
    addRect(ui, m_font, cx, cy + gap - 7, 2, 7, accent);
    addRect(ui, m_font, cx - gap, cy, 7, 2, accent);
    addRect(ui, m_font, cx + gap - 7, cy, 7, 2, accent);
    for (int i = 0; i < 12; ++i) {
        const float a = static_cast<float>(i) * 3.14159265f / 6.0f;
        addRect(ui, m_font, cx + std::cos(a) * 16.0f - 1.0f,
                cy + std::sin(a) * 16.0f - 1.0f, 2, 2, rgba(1.0f, 0.72f, 0.3f, 0.55f));
    }

    addRect(ui, m_font, 28, 24, 86, 28, rgba(1.0f, 0.55f, 0.16f, 0.12f));
    addBorder(ui, m_font, 28, 24, 86, 28, 1, rgba(1.0f, 0.62f, 0.18f, 0.7f));
    addText(ui, m_font, 36, 30, 15, rgba(1.0f, 0.72f, 0.3f), "V.O.I.D.");
    addText(ui, m_font, 126, 28, 15, rgba(0.98f, 0.95f, 0.88f), "PHYSICS LAB");
    addText(ui, m_font, 126, 46, 9, rgba(0.94f, 0.91f, 0.84f, 0.55f), "BOX3D // SECTOR 07");

    char stats[128];
    std::snprintf(stats, sizeof(stats), "%d FPS   %d BODIES", static_cast<int>(m_fps), static_cast<int>(m_props.size()));
    addText(ui, m_font, W - 220, 27, 14, rgba(0.98f, 0.95f, 0.88f, 0.95f), stats);
    addText(ui, m_font, W - 220, 45, 10, rgba(0.94f, 0.91f, 0.84f, 0.6f), m_backend);

    if (m_held) {
        addTextCentered(ui, m_font, cx, cy + 26, 10, rgba(1.0f, 0.72f, 0.3f), "ЗАХВАТ АКТИВЕН");
    } else if (m_hover) {
        addTextCentered(ui, m_font, cx, cy + 26, 10, rgba(0.94f, 0.91f, 0.84f, 0.6f),
                        "ОБЪЕКТ ДОСТУПЕН");
    }

    addText(ui, m_font, 28, H - 112, 11, rgba(0.98f, 0.95f, 0.88f, 0.8f),
            "WASD ДВИЖЕНИЕ  ·  SHIFT БЕГ  ·  SPACE ПРЫЖОК");
    addText(ui, m_font, 28, H - 94, 11, rgba(0.98f, 0.95f, 0.88f, 0.8f),
            "ЛКМ СХВАТИТЬ / ОТПУСТИТЬ  ·  ПКМ ТОЛЧОК  ·  F ШВЫРНУТЬ");
    addText(ui, m_font, 28, H - 76, 11, rgba(0.98f, 0.95f, 0.88f, 0.8f),
            "КОЛЕСО ДИСТАНЦИЯ  ·  G СЛУЧАЙНЫЙ  ·  R СБРОС  ·  Q SLOW-MO");

    const char* labels[] = {"1 ЯЩИК", "2 БОЧКА", "3 ШАР", "4 ДОСКА", "5 КИРПИЧ", "6 КАНИСТРА", "7 ДОМИНО"};
    const float slotW = 66, slotGap = 4;
    const float total = 7 * slotW + 6 * slotGap;
    float sx = cx - total * 0.5f;
    for (int i = 0; i < 7; ++i) {
        addRect(ui, m_font, sx, H - 52, slotW, 32, rgba(0.03f, 0.045f, 0.05f, 0.72f));
        addBorder(ui, m_font, sx, H - 52, slotW, 32, 1, rgba(0.94f, 0.91f, 0.84f, 0.12f));
        addText(ui, m_font, sx + 7, H - 47, 10, rgba(1.0f, 0.72f, 0.3f), labels[i]);
        sx += slotW + slotGap;
    }

    if (m_toastTimer > 0.0f && !m_toast.empty()) {
        addRect(ui, m_font, cx - 190, H - 104, 380, 28, rgba(0.03f, 0.045f, 0.05f, 0.88f));
        addRect(ui, m_font, cx - 190, H - 104, 3, 28, rgba(1.0f, 0.62f, 0.18f));
        addTextCentered(ui, m_font, cx + 2, H - 98, 10, rgba(0.98f, 0.95f, 0.88f), m_toast);
    }

    if (m_slowmo) {
        addTextCentered(ui, m_font, cx, 26, 10, rgba(1.0f, 0.65f, 0.2f), "SLOW MOTION  ×0.28");
    }
    if (m_muted) {
        addText(ui, m_font, W - 120, H - 30, 8, rgba(0.94f, 0.91f, 0.84f, 0.35f), "ЗВУК ВЫКЛ.");
    }
}

void Game::buildPauseUI(rhi::UIList& ui) {
    const float W = static_cast<float>(m_window.width());
    const float H = static_cast<float>(m_window.height());
    addRect(ui, m_font, 0, 0, W, H, rgba(0.01f, 0.015f, 0.018f, 0.62f));
    const float pw = 470, ph = 238;
    const float px = (W - pw) * 0.5f, py = (H - ph) * 0.5f;
    addRect(ui, m_font, px, py, pw, ph, rgba(0.035f, 0.045f, 0.05f, 0.96f));
    addBorder(ui, m_font, px, py, pw, ph, 1, rgba(0.94f, 0.91f, 0.84f, 0.16f));
    addRect(ui, m_font, px, py, 90, 2, rgba(1.0f, 0.62f, 0.18f));
    addText(ui, m_font, px + 34, py + 30, 11, rgba(1.0f, 0.65f, 0.2f), "СИМУЛЯЦИЯ ПРИОСТАНОВЛЕНА");
    addText(ui, m_font, px + 34, py + 58, 44, rgba(0.96f, 0.93f, 0.86f), "ПАУЗА");
    const float bx = px + 34, by = py + 142, bw = pw - 68, bh = 48;
    addRect(ui, m_font, bx, by, bw, bh, rgba(1.0f, 0.55f, 0.16f, 0.16f));
    addBorder(ui, m_font, bx, by, bw, bh, 1, rgba(1.0f, 0.65f, 0.2f, 0.8f));
    addTextCentered(ui, m_font, bx + bw * 0.5f, by + 15, 12, rgba(1.0f, 0.84f, 0.62f), "ПРОДОЛЖИТЬ");
    addText(ui, m_font, px + 34, py + 208, 8, rgba(0.94f, 0.91f, 0.84f, 0.35f),
            "ESC ПРОДОЛЖИТЬ  ·  R СБРОС");
}

void Game::frame(float rawDt) {
    m_time += rawDt;
    m_menuTime += rawDt;
    ++m_frameCount;
    m_fpsFrames++;
    m_fpsTimer += rawDt;
    if (m_fpsTimer >= 0.5f) {
        m_fps = static_cast<float>(m_fpsFrames) / m_fpsTimer;
        m_fpsFrames = 0;
        m_fpsTimer = 0.0f;
    }
    if (m_toastTimer > 0.0f) m_toastTimer -= rawDt;

    Input& input = *m_input;
    if (m_state == State::Menu) {
        m_window.setRelativeMouse(false);
        if (input.pressed(SDL_SCANCODE_RETURN) || input.mousePressed(1)) {
            const float by = 442.0f;
            const bool hitButton = input.mouseX() >= 64 && input.mouseX() <= 454 &&
                                   input.mouseY() >= by && input.mouseY() <= by + 54;
            if (input.pressed(SDL_SCANCODE_RETURN) || hitButton) {
                m_audio.click();
                startGame();
            }
        }
    } else if (m_state == State::Playing) {
        m_camera.yaw -= input.mouseDeltaX() * 0.0022f;
        m_camera.pitch -= input.mouseDeltaY() * 0.0022f;
        m_camera.pitch = glm::clamp(m_camera.pitch, -1.45f, 1.45f);

        if (input.pressed(SDL_SCANCODE_ESCAPE)) {
            pauseGame();
        } else if (input.pressed(SDL_SCANCODE_R)) {
            resetScene();
        } else if (input.pressed(SDL_SCANCODE_Q)) {
            m_slowmo = !m_slowmo;
            m_toast = m_slowmo ? "ЗАМЕДЛЕНИЕ ВРЕМЕНИ ×0.28" : "НОРМАЛЬНАЯ СКОРОСТЬ";
            m_toastTimer = 1.4f;
        } else if (input.pressed(SDL_SCANCODE_M)) {
            m_muted = !m_muted;
            m_toast = m_muted ? "ЗВУК ВЫКЛЮЧЕН" : "ЗВУК ВКЛЮЧЁН";
            m_toastTimer = 1.2f;
        } else if (input.pressed(SDL_SCANCODE_F)) {
            releaseHeld(true);
        } else if (input.pressed(SDL_SCANCODE_G)) {
            spawnRandom();
        } else {
            const SDL_Scancode digits[] = {SDL_SCANCODE_1, SDL_SCANCODE_2, SDL_SCANCODE_3,
                                           SDL_SCANCODE_4, SDL_SCANCODE_5, SDL_SCANCODE_6,
                                           SDL_SCANCODE_7};
            for (int i = 0; i < 7; ++i) {
                if (input.pressed(digits[i])) spawnProp(static_cast<PropKind>(i));
            }
        }

        m_timeScale += ((m_slowmo ? 0.28f : 1.0f) - m_timeScale) * (1.0f - std::exp(-7.0f * rawDt));
        const float dt = rawDt * m_timeScale;
        updatePlayer(dt);
        updateHover();

        if (input.mousePressed(1)) {
            m_keepHeld = false;
            tryGrab();
        }
        if (!input.mouseDown(1) && m_held && !m_keepHeld) releaseHeld(false);
        if (input.mousePressed(3)) {
            if (m_held) releaseHeld(true);
            else shove();
        }
        if (m_held && input.wheel() != 0.0f) {
            m_holdDistance = glm::clamp(m_holdDistance + input.wheel() * 0.35f, 1.3f, 5.5f);
        }
        updateGrab(dt);

        m_accumulator += dt;
        int steps = 0;
        while (m_accumulator >= kFixedStep && steps < 4) {
            m_physics.step(kFixedStep);
            m_accumulator -= kFixedStep;
            ++steps;
        }
        if (steps == 4) m_accumulator = 0.0f;
        syncProps();
        for (const auto& hit : m_physics.pollHits()) onHit(hit.tagA, hit.tagB, hit.approachSpeed);
    } else if (m_state == State::Paused) {
        if (input.pressed(SDL_SCANCODE_ESCAPE) || input.pressed(SDL_SCANCODE_RETURN)) {
            m_audio.click();
            resumeGame();
        }
        if (input.mousePressed(1)) {
            const float W = static_cast<float>(m_window.width());
            const float H = static_cast<float>(m_window.height());
            const float px = (W - 470) * 0.5f, py = (H - 238) * 0.5f;
            if (input.mouseX() >= px + 34 && input.mouseX() <= px + 470 - 34 &&
                input.mouseY() >= py + 142 && input.mouseY() <= py + 190) {
                resumeGame();
            }
        }
    }

    if (!m_window.focused() && m_state == State::Playing) pauseGame();

    if (m_state == State::Menu) {
        m_camera = m_menuCamera;
    }

    rhi::FrameData frameData;
    rhi::DrawList draws;
    rhi::UIList ui;
    buildFrame(frameData, draws, ui);
    m_device.render(frameData, draws, ui);
}

} // namespace

int run(int argc, char** argv) {
    Options options = parseOptions(argc, argv);

    SDL_SetAppMetadata("V.O.I.D. Engine", "0.1.0", "dev.void.engine");
    if (!SDL_Init(SDL_INIT_VIDEO | SDL_INIT_EVENTS | SDL_INIT_AUDIO)) {
        std::fprintf(stderr, "SDL_Init failed: %s\n", SDL_GetError());
        return 1;
    }
    Paths::initialize();
    log::init((Paths::baseDir() / "void.log").string());
    log::info("V.O.I.D. Engine 0.1.0 booting (base: %s)", Paths::baseDir().string().c_str());

    const rhi::Api api = chooseApi(options);
    if (api == rhi::Api::Auto) {
        log::error("No graphics backend is compiled in");
        return 1;
    }

    Window window;
    WindowOptions windowOptions;
    windowOptions.title = "V.O.I.D. Engine — PHYSICS LAB";
    windowOptions.width = options.width;
    windowOptions.height = options.height;
    windowOptions.fullscreen = options.fullscreen;
    windowOptions.vulkan = api == rhi::Api::Vulkan;
    if (!window.create(windowOptions)) return 1;

    std::unique_ptr<rhi::Device> device;
    if (api == rhi::Api::DX12) {
#ifdef VOID_WITH_DX12
        device = rhi::createDX12Device();
#endif
    } else if (api == rhi::Api::Vulkan) {
#ifdef VOID_WITH_VULKAN
        device = rhi::createVulkanDevice();
#endif
    }
    if (!device) {
        log::error("Requested graphics backend is not available: %s", rhi::apiName(api));
        return 1;
    }

    rhi::DeviceDesc desc;
    desc.window = window.handle();
    desc.width = window.width();
    desc.height = window.height();
    desc.vsync = options.vsync;
    desc.validation = options.validation;
    desc.adapter = options.adapter;
    if (!device->initialize(desc)) {
        log::error("Graphics device initialization failed");
        return 1;
    }

    Input input;
    Game game(window, *device, options);
    game.setInput(&input);
    if (!game.initialize()) {
        log::error("Game initialization failed");
        device->shutdown();
        return 1;
    }
    if (options.selftest) {
        const bool ok = game.selfTest();
        game.shutdown();
        device->shutdown();
        window.destroy();
        SDL_Quit();
        log::shutdown();
        return ok ? 0 : 2;
    }
    if (options.showcase) game.showcase();
    else if (options.autostart) game.startNow();

    u64 last = SDL_GetPerformanceCounter();
    u32 frames = 0;
    while (!window.shouldClose()) {
        input.beginFrame();
        SDL_Event event;
        while (SDL_PollEvent(&event)) {
            input.processEvent(event);
            window.processEvent(event);
        }
        if (window.resized() && !window.minimized()) {
            device->resize(window.width(), window.height());
            window.clearResized();
        }
        if (window.minimized()) {
            SDL_Delay(16);
            continue;
        }

        const u64 now = SDL_GetPerformanceCounter();
        const float dt = static_cast<float>(static_cast<double>(now - last) /
                                            static_cast<double>(SDL_GetPerformanceFrequency()));
        last = now;
        game.frame(std::min(dt, 0.05f));
        ++frames;
        if (options.smokeFrames > 0 && frames >= options.smokeFrames) break;
    }

    log::info("Shutdown: %u frames, backend %s", frames, device->adapterName());
    game.shutdown();
    device->shutdown();
    window.destroy();
    SDL_Quit();
    log::shutdown();
    return 0;
}

} // namespace vd::demo
