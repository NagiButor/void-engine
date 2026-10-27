#include "void/physics/PhysicsWorld.h"

#include "void/core/Log.h"

#include <glm/gtc/type_ptr.hpp>

#include <cmath>
#include <cstring>

namespace vd::physics {
namespace {
b3Vec3 toB3(glm::vec3 v) { return b3Vec3{v.x, v.y, v.z}; }
b3Quat toB3(glm::quat q) { return b3Quat{b3Vec3{q.x, q.y, q.z}, q.w}; }
glm::vec3 fromB3(b3Vec3 v) { return {v.x, v.y, v.z}; }
glm::quat fromB3(b3Quat q) { return {q.s, q.v.x, q.v.y, q.v.z}; }

void* tagPtr(u32 tag) { return reinterpret_cast<void*>(static_cast<uintptr_t>(tag)); }
u32 ptrTag(const void* ptr) { return static_cast<u32>(reinterpret_cast<uintptr_t>(ptr)); }

u64 categoryMask(Category category) {
    switch (category) {
    case Category::Static: return static_cast<u64>(Category::Prop) | static_cast<u64>(Category::Player);
    case Category::Player: return static_cast<u64>(Category::Static) | static_cast<u64>(Category::Prop);
    case Category::Prop:
    default: return static_cast<u64>(Category::Static) | static_cast<u64>(Category::Prop) | static_cast<u64>(Category::Player);
    }
}

void makeShapeDef(b3ShapeDef& def, float density, float friction, float restitution,
                  u32 tag, Category category, bool hitEvents) {
    def = b3DefaultShapeDef();
    def.density = density;
    def.baseMaterial.friction = friction;
    def.baseMaterial.restitution = restitution;
    def.filter.categoryBits = static_cast<u64>(category);
    def.filter.maskBits = categoryMask(category);
    def.filter.groupIndex = 0;
    def.userData = tagPtr(tag);
    def.enableHitEvents = hitEvents;
    def.enableContactEvents = false;
    def.enableSensorEvents = false;
}
} // namespace

bool Body::valid() const {
    return b3Body_IsValid(id) && b3Shape_IsValid(shape);
}

PhysicsWorld::~PhysicsWorld() {
    shutdown();
}

bool PhysicsWorld::initialize() {
    shutdown();
    b3WorldDef def = b3DefaultWorldDef();
    def.gravity = b3Vec3{0.0f, -9.82f, 0.0f};
    def.enableSleep = true;
    def.enableContinuous = true;
    def.hitEventThreshold = 0.8f;
    def.maximumLinearSpeed = 40.0f;
    m_world = b3CreateWorld(&def);
    if (!b3World_IsValid(m_world)) {
        log::error("Box3D world creation failed");
        m_world = {};
        return false;
    }
    log::info("Box3D world initialized");
    return true;
}

void PhysicsWorld::shutdown() {
    if (b3World_IsValid(m_world)) {
        b3DestroyWorld(m_world);
    }
    m_world = {};
    m_bodyCount = 0;
}

void PhysicsWorld::step(float dt) {
    if (b3World_IsValid(m_world)) b3World_Step(m_world, dt, 4);
}

void PhysicsWorld::settle(int steps) {
    for (int i = 0; i < steps; ++i) step(1.0f / 60.0f);
}

Body PhysicsWorld::createBody(bool dynamic, glm::vec3 position, glm::quat rotation, u32 tag,
                              Category category, bool hitEvents, bool player) {
    b3BodyDef def = b3DefaultBodyDef();
    def.type = dynamic ? b3_dynamicBody : b3_staticBody;
    def.position = toB3(position);
    def.rotation = toB3(rotation);
    def.linearDamping = 0.02f;
    def.angularDamping = 0.12f;
    def.userData = tagPtr(tag);
    def.enableSleep = true;
    def.enableContactRecycling = !player;
    if (player) {
        def.motionLocks.angularX = true;
        def.motionLocks.angularY = true;
        def.motionLocks.angularZ = true;
    }
    b3BodyId id = b3CreateBody(m_world, &def);
    ++m_bodyCount;
    Body body;
    body.id = id;
    body.tag = tag;
    body.dynamic = dynamic;
    (void)hitEvents;
    (void)category;
    return body;
}

Body PhysicsWorld::createBox(bool dynamic, glm::vec3 halfExtents, glm::vec3 position,
                             glm::quat rotation, float density, float friction, float restitution,
                             u32 tag, Category category, bool hitEvents) {
    Body body = createBody(dynamic, position, rotation, tag, category, hitEvents);
    b3ShapeDef def;
    makeShapeDef(def, dynamic ? density : 0.0f, friction, restitution, tag, category, hitEvents && dynamic);
    b3BoxHull hull = b3MakeBoxHull(halfExtents.x, halfExtents.y, halfExtents.z);
    body.shape = b3CreateHullShape(body.id, &def, &hull.base);
    return body;
}

Body PhysicsWorld::createSphere(bool dynamic, float radius, glm::vec3 position,
                                float density, float friction, float restitution, u32 tag,
                                Category category, bool hitEvents) {
    Body body = createBody(dynamic, position, glm::quat{1, 0, 0, 0}, tag, category, hitEvents);
    b3ShapeDef def;
    makeShapeDef(def, dynamic ? density : 0.0f, friction, restitution, tag, category, hitEvents && dynamic);
    b3Sphere sphere{};
    sphere.center = b3Vec3{0, 0, 0};
    sphere.radius = radius;
    body.shape = b3CreateSphereShape(body.id, &def, &sphere);
    return body;
}

Body PhysicsWorld::createCapsule(bool dynamic, float radius, float height, glm::vec3 position,
                                 float density, float friction, float restitution, u32 tag,
                                 Category category, bool hitEvents) {
    Body body = createBody(dynamic, position, glm::quat{1, 0, 0, 0}, tag, category, hitEvents, category == Category::Player);
    b3ShapeDef def;
    makeShapeDef(def, dynamic ? density : 0.0f, friction, restitution, tag, category, hitEvents && dynamic);
    b3Capsule capsule{};
    const float half = height * 0.5f;
    capsule.center1 = b3Vec3{0, -half, 0};
    capsule.center2 = b3Vec3{0, half, 0};
    capsule.radius = radius;
    body.shape = b3CreateCapsuleShape(body.id, &def, &capsule);
    return body;
}

Body PhysicsWorld::createCylinderHull(bool dynamic, float radius, float height, glm::vec3 position,
                                      glm::quat rotation, float density, float friction, float restitution,
                                      u32 tag, Category category, bool hitEvents) {
    constexpr int segments = 16;
    std::vector<b3Vec3> points;
    points.reserve(segments * 2);
    const float half = height * 0.5f;
    for (int i = 0; i < segments; ++i) {
        const float a = (static_cast<float>(i) / segments) * 6.28318530718f;
        const float x = std::cos(a) * radius;
        const float z = std::sin(a) * radius;
        points.push_back(b3Vec3{x, -half, z});
        points.push_back(b3Vec3{x, half, z});
    }
    b3HullData* hull = b3CreateHull(points.data(), static_cast<int>(points.size()), 48);
    if (!hull) {
        log::error("Box3D hull creation failed (cylinder)");
        return {};
    }
    Body body = createBody(dynamic, position, rotation, tag, category, hitEvents);
    b3ShapeDef def;
    makeShapeDef(def, dynamic ? density : 0.0f, friction, restitution, tag, category, hitEvents && dynamic);
    body.shape = b3CreateHullShape(body.id, &def, hull);
    b3DestroyHull(hull);
    return body;
}

Body PhysicsWorld::createConeHull(bool dynamic, float radius, float height, glm::vec3 position,
                                  glm::quat rotation, float density, float friction, float restitution,
                                  u32 tag, Category category, bool hitEvents) {
    constexpr int segments = 16;
    std::vector<b3Vec3> points;
    points.reserve(segments + 2);
    const float half = height * 0.5f;
    for (int i = 0; i < segments; ++i) {
        const float a = (static_cast<float>(i) / segments) * 6.28318530718f;
        points.push_back(b3Vec3{std::cos(a) * radius, -half, std::sin(a) * radius});
    }
    points.push_back(b3Vec3{0, half, 0});
    b3HullData* hull = b3CreateHull(points.data(), static_cast<int>(points.size()), 32);
    if (!hull) return {};
    Body body = createBody(dynamic, position, rotation, tag, category, hitEvents);
    b3ShapeDef def;
    makeShapeDef(def, dynamic ? density : 0.0f, friction, restitution, tag, category, hitEvents && dynamic);
    body.shape = b3CreateHullShape(body.id, &def, hull);
    b3DestroyHull(hull);
    return body;
}

Body PhysicsWorld::createPlayer(glm::vec3 position) {
    constexpr float radius = 0.35f;
    constexpr float segment = 0.9f;
    constexpr float density = 150.0f;
    return createCapsule(true, radius, segment, position, density, 0.0f, 0.0f, 1,
                         Category::Player, false);
}

void PhysicsWorld::destroyBody(Body& body) {
    if (b3Body_IsValid(body.id)) {
        b3DestroyBody(body.id);
        --m_bodyCount;
    }
    body = {};
}

void PhysicsWorld::setTransform(Body& body, glm::vec3 position, glm::quat rotation, bool wake) {
    if (!b3Body_IsValid(body.id)) return;
    b3Body_SetTransform(body.id, toB3(position), toB3(rotation));
    if (wake) b3Body_SetAwake(body.id, true);
}

glm::vec3 PhysicsWorld::getPosition(const Body& body) const {
    if (!b3Body_IsValid(body.id)) return {};
    return fromB3(b3Body_GetPosition(body.id));
}

glm::quat PhysicsWorld::getRotation(const Body& body) const {
    if (!b3Body_IsValid(body.id)) return glm::quat{1, 0, 0, 0};
    return fromB3(b3Body_GetRotation(body.id));
}

glm::vec3 PhysicsWorld::getLinearVelocity(const Body& body) const {
    if (!b3Body_IsValid(body.id)) return {};
    return fromB3(b3Body_GetLinearVelocity(body.id));
}

void PhysicsWorld::setLinearVelocity(Body& body, glm::vec3 velocity) {
    if (b3Body_IsValid(body.id)) b3Body_SetLinearVelocity(body.id, toB3(velocity));
}

void PhysicsWorld::setAngularVelocity(Body& body, glm::vec3 velocity) {
    if (b3Body_IsValid(body.id)) b3Body_SetAngularVelocity(body.id, toB3(velocity));
}

void PhysicsWorld::applyForce(Body& body, glm::vec3 force) {
    if (b3Body_IsValid(body.id)) b3Body_ApplyForceToCenter(body.id, toB3(force), true);
}

void PhysicsWorld::applyImpulse(Body& body, glm::vec3 impulse) {
    if (b3Body_IsValid(body.id)) b3Body_ApplyLinearImpulseToCenter(body.id, toB3(impulse), true);
}

void PhysicsWorld::applyImpulseAt(Body& body, glm::vec3 impulse, glm::vec3 worldPoint) {
    if (b3Body_IsValid(body.id)) {
        b3Body_ApplyLinearImpulse(body.id, toB3(impulse), toB3(worldPoint), true);
    }
}

float PhysicsWorld::getMass(const Body& body) const {
    return b3Body_IsValid(body.id) ? b3Body_GetMass(body.id) : 0.0f;
}

void PhysicsWorld::setAwake(Body& body, bool awake) {
    if (b3Body_IsValid(body.id)) b3Body_SetAwake(body.id, awake);
}

RayHit PhysicsWorld::raycast(glm::vec3 origin, glm::vec3 direction, float maxDistance, u64 mask) const {
    RayHit out;
    if (!b3World_IsValid(m_world)) return out;
    const float length = glm::length(direction);
    if (length < 1e-6f) return out;
    const glm::vec3 dir = direction / length;
    const glm::vec3 translation = dir * maxDistance;

    b3QueryFilter filter = b3DefaultQueryFilter();
    filter.categoryBits = ~0ull;
    filter.maskBits = mask;
    const b3RayResult result = b3World_CastRayClosest(
        m_world, b3Pos{origin.x, origin.y, origin.z}, toB3(translation), filter);
    if (!result.hit) return out;

    out.hit = true;
    out.point = fromB3(result.point);
    out.normal = fromB3(result.normal);
    out.distance = result.fraction * maxDistance;
    if (b3Shape_IsValid(result.shapeId)) {
        const b3BodyId bodyId = b3Shape_GetBody(result.shapeId);
        out.body.id = bodyId;
        out.body.shape = result.shapeId;
        out.body.tag = ptrTag(b3Body_GetUserData(bodyId));
        out.body.dynamic = b3Body_GetType(bodyId) == b3_dynamicBody;
    }
    return out;
}

std::vector<HitEvent> PhysicsWorld::pollHits() {
    std::vector<HitEvent> out;
    if (!b3World_IsValid(m_world)) return out;
    const b3ContactEvents events = b3World_GetContactEvents(m_world);
    for (int i = 0; i < events.hitCount; ++i) {
        const b3ContactHitEvent& e = events.hitEvents[i];
        if (!b3Shape_IsValid(e.shapeIdA) || !b3Shape_IsValid(e.shapeIdB)) continue;
        HitEvent hit;
        const b3BodyId bodyA = b3Shape_GetBody(e.shapeIdA);
        const b3BodyId bodyB = b3Shape_GetBody(e.shapeIdB);
        hit.tagA = ptrTag(b3Body_GetUserData(bodyA));
        hit.tagB = ptrTag(b3Body_GetUserData(bodyB));
        hit.point = fromB3(e.point);
        hit.normal = fromB3(e.normal);
        hit.approachSpeed = e.approachSpeed;
        out.push_back(hit);
    }
    return out;
}

ShapeFilter PhysicsWorld::getFilter(const Body& body) const {
    ShapeFilter out;
    if (!b3Shape_IsValid(body.shape)) return out;
    const b3Filter filter = b3Shape_GetFilter(body.shape);
    out.categoryBits = filter.categoryBits;
    out.maskBits = filter.maskBits;
    out.groupIndex = filter.groupIndex;
    return out;
}

void PhysicsWorld::setFilter(Body& body, const ShapeFilter& filter) {
    if (!b3Shape_IsValid(body.shape)) return;
    b3Filter b3filter;
    b3filter.categoryBits = filter.categoryBits;
    b3filter.maskBits = filter.maskBits;
    b3filter.groupIndex = filter.groupIndex;
    b3Shape_SetFilter(body.shape, b3filter, true);
}

} // namespace vd::physics
