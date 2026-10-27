#pragma once

#include <cstdint>
#include <glm/glm.hpp>
#include <glm/gtc/quaternion.hpp>
#include <glm/gtc/matrix_transform.hpp>

namespace vd {

using u8 = std::uint8_t;
using u16 = std::uint16_t;
using u32 = std::uint32_t;
using u64 = std::uint64_t;
using i32 = std::int32_t;

struct Transform {
    glm::vec3 position{0.0f};
    glm::quat rotation{1.0f, 0.0f, 0.0f, 0.0f};
    glm::vec3 scale{1.0f};

    glm::mat4 matrix() const {
        return glm::translate(glm::mat4(1.0f), position) *
               glm::mat4_cast(rotation) *
               glm::scale(glm::mat4(1.0f), scale);
    }
};

inline glm::vec3 toVec3(const glm::vec4& v) { return glm::vec3(v); }

} // namespace vd
