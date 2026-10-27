#include "demo/Demo.h"

#include "void/core/Log.h"
#include "void/core/Paths.h"
#include "void/platform/Input.h"
#include "void/platform/Window.h"
#include "void/rhi/RHI.h"

#include <SDL3/SDL.h>

#include <algorithm>
#include <chrono>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <string>

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
    u32 smokeFrames = 0;
};

bool parseBool(const char* value, bool fallback) {
    if (!value) return fallback;
    return std::strcmp(value, "1") == 0 || std::strcmp(value, "true") == 0 || std::strcmp(value, "on") == 0;
}

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
            options.width = static_cast<u32>(std::max(320, std::atoi(arg + 8)));
        } else if (std::strncmp(arg, "--height=", 9) == 0) {
            options.height = static_cast<u32>(std::max(240, std::atoi(arg + 9)));
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
            SDL_MESSAGEBOX_INFORMATION,
            nullptr,
            "V.O.I.D. Engine",
            "Select a graphics backend for this session.",
            SDL_arraysize(buttons),
            buttons,
            nullptr,
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
    if (!window.create(windowOptions)) {
        return 1;
    }

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
    u64 last = SDL_GetPerformanceCounter();
    u32 frames = 0;
    u32 fpsFrames = 0;
    double fpsTime = 0.0;
    double fps = 0.0;

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

        rhi::FrameData frame;
        rhi::DrawList draws;
        rhi::UIList ui;
        device->render(frame, draws, ui);

        const u64 now = SDL_GetPerformanceCounter();
        const double dt = static_cast<double>(now - last) / static_cast<double>(SDL_GetPerformanceFrequency());
        last = now;
        ++frames;
        ++fpsFrames;
        fpsTime += dt;
        if (fpsTime >= 0.5) {
            fps = fpsFrames / fpsTime;
            fpsFrames = 0;
            fpsTime = 0.0;
        }

        if (options.smokeFrames > 0 && frames >= options.smokeFrames) {
            break;
        }
    }

    log::info("Shutdown: %u frames, %.1f fps, adapter: %s", frames, fps, device->adapterName());
    device->shutdown();
    window.destroy();
    SDL_Quit();
    log::shutdown();
    return 0;
}

} // namespace vd::demo
