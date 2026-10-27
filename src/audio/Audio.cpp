#include "void/audio/Audio.h"

#include "void/core/Log.h"

#include <algorithm>
#include <cmath>
#include <random>

namespace vd {
namespace {
constexpr int kSampleRate = 48000;
constexpr int kChannels = 2;
constexpr float kPi = 3.14159265358979323846f;
std::mt19937 g_rng(0x51A7u);
} // namespace

AudioKit::~AudioKit() {
    shutdown();
}

bool AudioKit::init() {
    if (m_stream) return true;
    SDL_AudioSpec spec{};
    spec.format = SDL_AUDIO_F32;
    spec.channels = kChannels;
    spec.freq = kSampleRate;
    m_stream = SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, nullptr, nullptr);
    if (!m_stream) {
        log::warn("Audio: no playback device (%s)", SDL_GetError());
        return false;
    }
    SDL_SetAudioStreamGain(m_stream, m_muted ? 0.0f : m_gain);
    SDL_ResumeAudioStreamDevice(m_stream);
    log::info("Audio: %d Hz stereo synth ready", kSampleRate);
    return true;
}

void AudioKit::shutdown() {
    if (m_stream) {
        SDL_DestroyAudioStream(m_stream);
        m_stream = nullptr;
    }
}

void AudioKit::setMuted(bool muted) {
    m_muted = muted;
    if (m_stream) SDL_SetAudioStreamGain(m_stream, muted ? 0.0f : m_gain);
}

float AudioKit::noise() {
    return (static_cast<float>(g_rng() & 0xFFFF) / 32768.0f) - 1.0f;
}

void AudioKit::push(const std::vector<float>& samples) {
    if (!m_stream || m_muted || samples.empty()) return;
    const float queuedSeconds = static_cast<float>(SDL_GetAudioStreamQueued(m_stream)) /
                                static_cast<float>(kSampleRate * kChannels * sizeof(float));
    if (queuedSeconds > 0.35f) SDL_ClearAudioStream(m_stream);
    SDL_PutAudioStreamData(m_stream, samples.data(), static_cast<int>(samples.size() * sizeof(float)));
}

std::vector<float> AudioKit::makeTone(float fromHz, float toHz, float duration, float gain,
                                      bool addNoise, float noiseGain) {
    const int frames = static_cast<int>(duration * kSampleRate);
    std::vector<float> out(static_cast<size_t>(frames) * kChannels, 0.0f);
    float phase = 0.0f;
    for (int i = 0; i < frames; ++i) {
        const float t = static_cast<float>(i) / kSampleRate;
        const float u = static_cast<float>(i) / std::max(1, frames - 1);
        const float env = std::exp(-t * 7.0f) * std::min(1.0f, u * 20.0f);
        const float hz = fromHz + (toHz - fromHz) * u;
        phase += (2.0f * kPi * hz) / kSampleRate;
        float sample = std::sin(phase) * gain * env;
        if (addNoise) sample += noise() * noiseGain * env;
        out[static_cast<size_t>(i) * 2] = sample;
        out[static_cast<size_t>(i) * 2 + 1] = sample;
    }
    return out;
}

std::vector<float> AudioKit::makeImpact(const std::string& material, float strength) {
    const bool metal = material == "metal";
    const bool concrete = material == "concrete";
    const float duration = metal ? 0.38f : (concrete ? 0.22f : 0.28f);
    const float base = metal ? 180.0f : (concrete ? 70.0f : 105.0f);
    const float gain = 0.06f + 0.35f * strength;
    auto out = makeTone(base, base * 0.55f, duration, gain * 0.6f, false, 0.0f);
    const int frames = static_cast<int>(duration * kSampleRate);
    for (int i = 0; i < frames; ++i) {
        const float t = static_cast<float>(i) / kSampleRate;
        const float env = std::exp(-t * (metal ? 5.0f : 14.0f));
        float n = noise() * 0.18f * strength * env;
        if (metal) {
            n += std::sin(2.0f * kPi * (740.0f + 180.0f * std::sin(t * 13.0f)) * t) * 0.08f * strength * env;
        }
        out[static_cast<size_t>(i) * 2] += n;
        out[static_cast<size_t>(i) * 2 + 1] += n;
    }
    return out;
}

void AudioKit::impact(const std::string& material, float strength) {
    if (m_muted || !m_stream) return;
    const float now = static_cast<float>(SDL_GetTicks()) * 0.001f;
    if (now - m_lastImpact < 0.04f) return;
    m_lastImpact = now;
    push(makeImpact(material, std::clamp(strength, 0.03f, 1.0f)));
}

void AudioKit::zap() {
    auto out = makeTone(140.0f, 620.0f, 0.16f, 0.18f, true, 0.05f);
    push(out);
}

void AudioKit::throwSound() {
    auto out = makeTone(300.0f, 1400.0f, 0.22f, 0.06f, true, 0.2f);
    push(out);
}

void AudioKit::spawn() {
    auto out = makeTone(300.0f, 760.0f, 0.1f, 0.12f, false, 0.0f);
    push(out);
}

void AudioKit::slowmo(bool active) {
    auto out = active ? makeTone(220.0f, 55.0f, 0.55f, 0.1f, true, 0.03f)
                      : makeTone(60.0f, 220.0f, 0.4f, 0.1f, false, 0.0f);
    push(out);
}

void AudioKit::click() {
    auto out = makeTone(520.0f, 700.0f, 0.07f, 0.08f, false, 0.0f);
    push(out);
}

} // namespace vd
