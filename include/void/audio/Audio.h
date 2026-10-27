#pragma once

#include <SDL3/SDL.h>
#include <string>
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

private:
    void push(const std::vector<float>& samples);
    std::vector<float> makeImpact(const std::string& material, float strength);
    std::vector<float> makeTone(float fromHz, float toHz, float duration, float gain,
                                bool noise, float noiseGain);
    float noise();

    SDL_AudioStream* m_stream = nullptr;
    float m_gain = 0.55f;
    bool m_muted = false;
    double m_time = 0.0;
    float m_lastImpact = 0.0f;
};

} // namespace vd
