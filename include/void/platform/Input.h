#pragma once

#include "void/core/Types.h"

#include <SDL3/SDL.h>
#include <array>
#include <cstring>

namespace vd {

class Input {
public:
    Input();

    void beginFrame();
    void processEvent(const SDL_Event& event);

    bool down(SDL_Scancode key) const { return m_keys[key]; }
    bool pressed(SDL_Scancode key) const { return m_pressed[key]; }
    bool released(SDL_Scancode key) const { return m_released[key]; }

    bool mouseDown(u8 button) const { return button < 5 && m_mouse[button]; }
    bool mousePressed(u8 button) const { return button < 5 && m_mousePressed[button]; }
    bool mouseReleased(u8 button) const { return button < 5 && m_mouseReleased[button]; }

    float mouseDeltaX() const { return m_deltaX; }
    float mouseDeltaY() const { return m_deltaY; }
    float wheel() const { return m_wheel; }
    float mouseX() const { return m_mouseX; }
    float mouseY() const { return m_mouseY; }

    void setMousePosition(float x, float y) { m_mouseX = x; m_mouseY = y; }

private:
    std::array<bool, SDL_SCANCODE_COUNT> m_keys{};
    std::array<bool, SDL_SCANCODE_COUNT> m_pressed{};
    std::array<bool, SDL_SCANCODE_COUNT> m_released{};
    std::array<bool, 5> m_mouse{};
    std::array<bool, 5> m_mousePressed{};
    std::array<bool, 5> m_mouseReleased{};
    float m_deltaX = 0.0f;
    float m_deltaY = 0.0f;
    float m_wheel = 0.0f;
    float m_mouseX = 0.0f;
    float m_mouseY = 0.0f;
};

} // namespace vd
