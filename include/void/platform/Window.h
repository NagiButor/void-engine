#pragma once

#include "void/core/Types.h"

#include <SDL3/SDL.h>
#include <string>

namespace vd {

struct WindowOptions {
    std::string title = "V.O.I.D. Engine";
    u32 width = 1600;
    u32 height = 900;
    bool fullscreen = false;
    bool vulkan = false;
};

class Window {
public:
    Window() = default;
    ~Window();

    bool create(const WindowOptions& options);
    void destroy();

    // Returns true when the window state changed in a way the caller cares about.
    bool processEvent(const SDL_Event& event);
    void setRelativeMouse(bool enabled);
    void showCursor(bool visible);
    void setTitle(const std::string& title);

    bool shouldClose() const { return m_shouldClose; }
    bool resized() const { return m_resized; }
    bool focused() const { return m_focused; }
    bool minimized() const { return m_minimized; }
    void clearResized() { m_resized = false; }

    SDL_Window* handle() const { return m_window; }
    u32 width() const { return m_width; }
    u32 height() const { return m_height; }
    void* nativeHandle() const;

private:
    SDL_Window* m_window = nullptr;
    u32 m_width = 0;
    u32 m_height = 0;
    bool m_shouldClose = false;
    bool m_resized = false;
    bool m_focused = true;
    bool m_minimized = false;
};

} // namespace vd
