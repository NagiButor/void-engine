#pragma once

#include <SDL3/SDL.h>

#include <random>
#include <string>
#include <unordered_map>
#include <vector>

namespace vd {

class AudioKit {
public:
    AudioKit() = default;
    ~AudioKit();

    bool init();
    void shutdown();
    void setMuted(bool muted);
    bool muted() const { return m_muted; }

    void impact(const std::string& material, float strength);
    void zap();
    void throwSound();
    void spawn();
    void slowmo(bool active);
    void click();
    void footstep(float speed);

private:
    struct Clip {
        std::vector<float> samples; // interleaved stereo, 48 kHz
    };

    bool loadClip(const std::string& name, const std::string& path);
    bool play(const std::string& name, float gain, float pitch = 1.0f,
              float maxQueuedSeconds = 0.45f);
    void push(const std::vector<float>& samples);
    std::vector<float> makeImpact(const std::string& material, float strength);
    std::vector<float> makeTone(float fromHz, float toHz, float duration, float gain,
                                bool noise, float noiseGain);
    float noise();
    float randomRange(float min, float max);

    SDL_AudioStream* m_stream = nullptr;
    std::unordered_map<std::string, Clip> m_clips;
    std::mt19937 m_rng{0x51A7u};
    float m_gain = 0.55f;
    bool m_muted = false;
    float m_lastImpact = 0.0f;
    int m_footstepIndex = 0;
};

} // namespace vd
