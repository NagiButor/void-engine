#pragma once

#include <glm/glm.hpp>
#include <glm/gtc/matrix_transform.hpp>

namespace vd {

struct Camera {
    glm::vec3 position{0.0f, 1.6f, 9.6f};
    float yaw = 0.0f;
    float pitch = 0.0f;
    float fovDegrees = 78.0f;
    float aspect = 16.0f / 9.0f;
    float nearPlane = 0.045f;
    float farPlane = 200.0f;

    glm::vec3 forward() const {
        const float cp = std::cos(pitch);
        return glm::normalize(glm::vec3{-std::sin(yaw) * cp, std::sin(pitch), -std::cos(yaw) * cp});
    }

    glm::vec3 right() const {
        return glm::normalize(glm::vec3{std::cos(yaw), 0.0f, -std::sin(yaw)});
    }

    glm::mat4 view() const {
        return glm::lookAt(position, position + forward(), glm::vec3{0.0f, 1.0f, 0.0f});
    }

    glm::mat4 projection() const {
        return glm::perspectiveRH_ZO(glm::radians(fovDegrees), aspect, nearPlane, farPlane);
    }

    glm::mat4 viewProjection() const {
        return projection() * view();
    }

    glm::mat4 inverseViewProjection() const {
        return glm::inverse(viewProjection());
    }
};

} // namespace vd
