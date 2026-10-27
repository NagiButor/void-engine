#pragma once

#include "void/core/Types.h"

#include <box3d/box3d.h>
#include <glm/gtc/quaternion.hpp>

#include <array>
#include <cstdint>
#include <vector>

namespace vd::physics {

enum class Category : u64 {
    Static = 1ull << 0,
    Prop = 1ull << 1,
    Player = 1ull << 2,
};

struct Body {
    b3BodyId id{};
    b3ShapeId shape{};
    u32 tag = 0;
    bool dynamic = false;
    bool valid() const;
};

struct RayHit {
    bool hit = false;
    Body body;
    glm::vec3 point{0.0f};
    glm::vec3 normal{0.0f, 1.0f, 0.0f};
    float distance = 0.0f;
};

struct HitEvent {
    u32 tagA = 0;
    u32 tagB = 0;
    glm::vec3 point{0.0f};
    glm::vec3 normal{0.0f, 1.0f, 0.0f};
    float approachSpeed = 0.0f;
};

struct ShapeFilter {
    u64 categoryBits = 0;
    u64 maskBits = 0;
    int groupIndex = 0;
};

class PhysicsWorld {
public:
    PhysicsWorld() = default;
    ~PhysicsWorld();

    bool initialize();
    void shutdown();
    void step(float dt);
    void settle(int steps = 110);

    Body createBox(bool dynamic, glm::vec3 halfExtents, glm::vec3 position,
                   glm::quat rotation, float density, float friction, float restitution,
                   u32 tag, Category category = Category::Prop, bool hitEvents = true);
    Body createSphere(bool dynamic, float radius, glm::vec3 position,
                      float density, float friction, float restitution, u32 tag,
                      Category category = Category::Prop, bool hitEvents = true);
    Body createCapsule(bool dynamic, float radius, float height, glm::vec3 position,
                       float density, float friction, float restitution, u32 tag,
                       Category category, bool hitEvents = false);
    Body createCylinderHull(bool dynamic, float radius, float height, glm::vec3 position,
                            glm::quat rotation, float density, float friction, float restitution,
                            u32 tag, Category category = Category::Prop, bool hitEvents = true);
    Body createConeHull(bool dynamic, float radius, float height, glm::vec3 position,
                        glm::quat rotation, float density, float friction, float restitution,
                        u32 tag, Category category = Category::Prop, bool hitEvents = true);

    Body createPlayer(glm::vec3 position);
    void destroyBody(Body& body);

    void setTransform(Body& body, glm::vec3 position, glm::quat rotation, bool wake = true);
    glm::vec3 getPosition(const Body& body) const;
    glm::quat getRotation(const Body& body) const;
    glm::vec3 getLinearVelocity(const Body& body) const;
    void setLinearVelocity(Body& body, glm::vec3 velocity);
    void setAngularVelocity(Body& body, glm::vec3 velocity);
    void applyForce(Body& body, glm::vec3 force);
    void applyImpulse(Body& body, glm::vec3 impulse);
    void applyImpulseAt(Body& body, glm::vec3 impulse, glm::vec3 worldPoint);
    float getMass(const Body& body) const;
    void setAwake(Body& body, bool awake);

    RayHit raycast(glm::vec3 origin, glm::vec3 direction, float maxDistance,
                   u64 mask = static_cast<u64>(Category::Static) | static_cast<u64>(Category::Prop)) const;
    std::vector<HitEvent> pollHits();

    ShapeFilter getFilter(const Body& body) const;
    void setFilter(Body& body, const ShapeFilter& filter);

    b3WorldId rawWorld() const { return m_world; }
    int bodyCount() const { return m_bodyCount; }

private:
    Body createBody(bool dynamic, glm::vec3 position, glm::quat rotation, u32 tag,
                    Category category, bool hitEvents, bool player = false);

    b3WorldId m_world{};
    u32 m_bodyCount = 0;
};

} // namespace vd::physics
