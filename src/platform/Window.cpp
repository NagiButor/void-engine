#include "void/platform/Window.h"

#include "void/core/Log.h"

namespace vd {

Window::~Window() {
    destroy();
}

bool Window::create(const WindowOptions& options) {
    SDL_WindowFlags flags = SDL_WINDOW_RESIZABLE | SDL_WINDOW_HIGH_PIXEL_DENSITY;
    if (options.vulkan) flags |= SDL_WINDOW_VULKAN;
    if (options.fullscreen) flags |= SDL_WINDOW_FULLSCREEN;

    m_window = SDL_CreateWindow(options.title.c_str(),
                                static_cast<int>(options.width),
                                static_cast<int>(options.height),
                                flags);
    if (!m_window) {
        log::error("SDL_CreateWindow failed: %s", SDL_GetError());
        return false;
    }

    SDL_SetWindowMinimumSize(m_window, 960, 540);
    int width = static_cast<int>(options.width);
    int height = static_cast<int>(options.height);
    SDL_GetWindowSizeInPixels(m_window, &width, &height);
    m_width = static_cast<u32>(width);
    m_height = static_cast<u32>(height);
    log::info("Window created: %ux%u", m_width, m_height);
    return true;
}

void Window::destroy() {
    if (m_window) {
        SDL_DestroyWindow(m_window);
        m_window = nullptr;
    }
}

bool Window::processEvent(const SDL_Event& event) {
    switch (event.type) {
    case SDL_EVENT_QUIT:
        m_shouldClose = true;
        return true;
    case SDL_EVENT_WINDOW_CLOSE_REQUESTED:
        m_shouldClose = true;
        return true;
    case SDL_EVENT_WINDOW_RESIZED:
    case SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED: {
        m_resized = true;
        int width = 0;
        int height = 0;
        SDL_GetWindowSizeInPixels(m_window, &width, &height);
        m_width = static_cast<u32>(width);
        m_height = static_cast<u32>(height);
        return true;
    }
    case SDL_EVENT_WINDOW_FOCUS_GAINED:
        m_focused = true;
        return true;
    case SDL_EVENT_WINDOW_FOCUS_LOST:
        m_focused = false;
        return true;
    case SDL_EVENT_WINDOW_MINIMIZED:
        m_minimized = true;
        return true;
    case SDL_EVENT_WINDOW_RESTORED: {
        m_minimized = false;
        m_resized = true;
        int width = 0;
        int height = 0;
        SDL_GetWindowSizeInPixels(m_window, &width, &height);
        m_width = static_cast<u32>(width);
        m_height = static_cast<u32>(height);
        return true;
    }
    default:
        return false;
    }
}

void Window::setRelativeMouse(bool enabled) {
    if (m_window) {
        SDL_SetWindowRelativeMouseMode(m_window, enabled);
    }
}

void Window::showCursor(bool visible) {
    if (visible) SDL_ShowCursor();
    else SDL_HideCursor();
}

void Window::setTitle(const std::string& title) {
    if (m_window) SDL_SetWindowTitle(m_window, title.c_str());
}

void* Window::nativeHandle() const {
    if (!m_window) return nullptr;
    SDL_PropertiesID props = SDL_GetWindowProperties(m_window);
    return SDL_GetPointerProperty(props, SDL_PROP_WINDOW_WIN32_HWND_POINTER, nullptr);
}

} // namespace vd
