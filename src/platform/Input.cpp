#include "void/platform/Input.h"

namespace vd {

Input::Input() {
    m_keys.fill(false);
    m_pressed.fill(false);
    m_released.fill(false);
    m_mouse.fill(false);
    m_mousePressed.fill(false);
    m_mouseReleased.fill(false);
}

void Input::beginFrame() {
    m_pressed.fill(false);
    m_released.fill(false);
    m_mousePressed.fill(false);
    m_mouseReleased.fill(false);
    m_deltaX = 0.0f;
    m_deltaY = 0.0f;
    m_wheel = 0.0f;
}

void Input::processEvent(const SDL_Event& event) {
    switch (event.type) {
    case SDL_EVENT_KEY_DOWN: {
        const SDL_Scancode code = event.key.scancode;
        if (code < m_keys.size()) {
            if (!event.key.repeat) m_pressed[code] = true;
            m_keys[code] = true;
        }
        break;
    }
    case SDL_EVENT_KEY_UP: {
        const SDL_Scancode code = event.key.scancode;
        if (code < m_keys.size()) {
            m_keys[code] = false;
            m_released[code] = true;
        }
        break;
    }
    case SDL_EVENT_MOUSE_MOTION:
        m_deltaX += event.motion.xrel;
        m_deltaY += event.motion.yrel;
        m_mouseX = event.motion.x;
        m_mouseY = event.motion.y;
        break;
    case SDL_EVENT_MOUSE_BUTTON_DOWN:
        if (event.button.button < 5) {
            if (!m_mouse[event.button.button]) m_mousePressed[event.button.button] = true;
            m_mouse[event.button.button] = true;
        }
        break;
    case SDL_EVENT_MOUSE_BUTTON_UP:
        if (event.button.button < 5) {
            m_mouse[event.button.button] = false;
            m_mouseReleased[event.button.button] = true;
        }
        break;
    case SDL_EVENT_MOUSE_WHEEL:
        m_wheel += event.wheel.y;
        break;
    default:
        break;
    }
}

} // namespace vd
