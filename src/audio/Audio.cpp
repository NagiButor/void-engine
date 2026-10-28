#include "void/audio/Audio.h"

#include "void/core/Log.h"
#include "void/core/Paths.h"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstring>
#include <iterator>

namespace vd {
namespace {
constexpr int kSampleRate = 48000;
constexpr int kChannels = 2;
constexpr float kPi = 3.14159265358979323846f;
constexpr float kBytesPerSecond =
    static_cast<float>(kSampleRate * kChannels * static_cast<int>(sizeof(float)));
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

    static const char* kClipNames[] = {
        "impact_wood", "impact_metal", "impact_concrete", "impact_plastic",
        "grab", "throw", "spawn", "slowmo_on", "slowmo_off", "ui_click",
        "footstep_1", "footstep_2",
    };
    m_clips.reserve(std::size(kClipNames));
    for (const char* name : kClipNames) {
        const std::string path = Paths::asset(std::string("audio/sfx/") + name + ".wav").string();
        loadClip(name, path);
    }
    const int expected = static_cast<int>(std::size(kClipNames));
    if (m_clips.empty()) {
        log::warn("Audio: no sound-effect clips found, using synthesized fallback");
    } else if (static_cast<int>(m_clips.size()) < expected) {
        log::warn("Audio: loaded %d/%d clips; missing clips use the synthesized fallback",
                  static_cast<int>(m_clips.size()), expected);
    } else {
        log::info("Audio: %d Hz stereo, %d generated sound-effect clips",
                  kSampleRate, static_cast<int>(m_clips.size()));
    }
    return true;
}

void AudioKit::shutdown() {
    m_clips.clear();
    if (m_stream) {
        SDL_DestroyAudioStream(m_stream);
        m_stream = nullptr;
    }
}

void AudioKit::setMuted(bool muted) {
    m_muted = muted;
    if (m_stream) SDL_SetAudioStreamGain(m_stream, muted ? 0.0f : m_gain);
}

bool AudioKit::loadClip(const std::string& name, const std::string& path) {
    SDL_AudioSpec src{};
    Uint8* data = nullptr;
    Uint32 length = 0;
    if (!SDL_LoadWAV(path.c_str(), &src, &data, &length)) {
        log::debug("Audio: cannot load %s (%s)", path.c_str(), SDL_GetError());
        return false;
    }

    SDL_AudioSpec dst{};
    dst.format = SDL_AUDIO_F32;
    dst.channels = kChannels;
    dst.freq = kSampleRate;
    SDL_AudioStream* convert = SDL_CreateAudioStream(&src, &dst);
    if (!convert) {
        SDL_free(data);
        log::warn("Audio: cannot create converter for %s", path.c_str());
        return false;
    }
    SDL_PutAudioStreamData(convert, data, static_cast<int>(length));
    SDL_FlushAudioStream(convert);
    const int available = SDL_GetAudioStreamAvailable(convert);
    std::vector<float> samples(static_cast<size_t>(std::max(0, available)) / sizeof(float));
    int received = 0;
    if (!samples.empty()) {
        received = SDL_GetAudioStreamData(convert, samples.data(),
                                          static_cast<int>(samples.size() * sizeof(float)));
    }
    SDL_DestroyAudioStream(convert);
    SDL_free(data);
    if (received <= 0) {
        log::warn("Audio: empty clip %s", name.c_str());
        return false;
    }
    samples.resize(static_cast<size_t>(received) / sizeof(float));
    if (samples.size() < 2) return false;

    // Trim leading/trailing near-silence so impacts fire immediately, and keep
    // a short tail so reverb/decay is not cut off.
    constexpr float kSilence = 0.004f;
    size_t start = 0;
    while (start + 1 < samples.size() &&
           std::abs(samples[start]) < kSilence && std::abs(samples[start + 1]) < kSilence) {
        start += 2;
    }
    size_t end = samples.size();
    while (end > start + 2 &&
           std::abs(samples[end - 1]) < kSilence && std::abs(samples[end - 2]) < kSilence) {
        end -= 2;
    }
    start = start > 200 ? start - 200 : 0;
    end = std::min(samples.size(), end + kSampleRate / 24); // ~40 ms tail
    std::vector<float> trimmed(samples.begin() + static_cast<std::ptrdiff_t>(start),
                               samples.begin() + static_cast<std::ptrdiff_t>(end));

    float peak = 0.0f;
    for (float sample : trimmed) peak = std::max(peak, std::abs(sample));
    if (peak > 1e-4f) {
        const float scale = 0.9f / peak;
        for (float& sample : trimmed) sample *= scale;
    }
    if (trimmed.size() < 2) return false;
    m_clips[name] = Clip{std::move(trimmed)};
    return true;
}

float AudioKit::randomRange(float min, float max) {
    const float unit = static_cast<float>(m_rng() & 0xFFFFFF) / static_cast<float>(0x1000000);
    return min + (max - min) * unit;
}

bool AudioKit::play(const std::string& name, float gain, float pitch, float maxQueuedSeconds) {
    if (!m_stream || m_muted) return false;
    auto it = m_clips.find(name);
    if (it == m_clips.end() || it->second.samples.empty()) return false;
    const Clip& clip = it->second;

    if (maxQueuedSeconds > 0.0f) {
        const float queued = static_cast<float>(SDL_GetAudioStreamQueued(m_stream)) / kBytesPerSecond;
        if (queued > maxQueuedSeconds) SDL_ClearAudioStream(m_stream);
    }

    gain = std::clamp(gain, 0.0f, 1.5f);
    pitch = std::clamp(pitch, 0.5f, 2.0f);
    const size_t frames = clip.samples.size() / kChannels;
    if (frames == 0) return false;

    if (std::abs(pitch - 1.0f) < 0.001f && std::abs(gain - 1.0f) < 0.001f) {
        push(clip.samples);
        return true;
    }

    const size_t outFrames = std::max<size_t>(1, static_cast<size_t>(frames / pitch));
    std::vector<float> out(outFrames * kChannels);
    for (size_t i = 0; i < outFrames; ++i) {
        const float pos = static_cast<float>(i) * pitch;
        const size_t idx = std::min(static_cast<size_t>(pos), frames - 1);
        const size_t next = std::min(idx + 1, frames - 1);
        const float frac = pos - static_cast<float>(idx);
        for (int c = 0; c < kChannels; ++c) {
            const float a = clip.samples[idx * kChannels + c];
            const float b = clip.samples[next * kChannels + c];
            out[i * kChannels + c] = (a + (b - a) * frac) * gain;
        }
    }
    push(out);
    return true;
}

void AudioKit::push(const std::vector<float>& samples) {
    if (!m_stream || m_muted || samples.empty()) return;
    SDL_PutAudioStreamData(m_stream, samples.data(),
                           static_cast<int>(samples.size() * sizeof(float)));
}

void AudioKit::impact(const std::string& material, float strength) {
    if (m_muted || !m_stream) return;
    const float now = static_cast<float>(SDL_GetTicks()) * 0.001f;
    if (now - m_lastImpact < 0.04f) return;
    m_lastImpact = now;

    strength = std::clamp(strength, 0.03f, 1.0f);
    std::string clip = "impact_wood";
    if (material == "metal") clip = "impact_metal";
    else if (material == "concrete") clip = "impact_concrete";
    else if (material == "plastic") clip = "impact_plastic";

    const float gain = std::clamp(0.3f + 0.7f * std::sqrt(strength), 0.08f, 1.0f);
    const float pitch = randomRange(0.94f, 1.06f);
    if (play(clip, gain, pitch)) return;
    push(makeImpact(material, strength));
}

void AudioKit::zap() {
    if (!play("grab", 0.9f, randomRange(0.97f, 1.03f))) {
        push(makeTone(140.0f, 620.0f, 0.16f, 0.18f, true, 0.05f));
    }
}

void AudioKit::throwSound() {
    if (!play("throw", 0.95f, randomRange(0.97f, 1.03f))) {
        push(makeTone(300.0f, 1400.0f, 0.22f, 0.06f, true, 0.2f));
    }
}

void AudioKit::spawn() {
    if (!play("spawn", 0.85f, randomRange(0.97f, 1.04f))) {
        push(makeTone(300.0f, 760.0f, 0.1f, 0.12f, false, 0.0f));
    }
}

void AudioKit::slowmo(bool active) {
    if (!play(active ? "slowmo_on" : "slowmo_off", 0.55f)) {
        push(active ? makeTone(220.0f, 55.0f, 0.55f, 0.1f, true, 0.03f)
                    : makeTone(60.0f, 220.0f, 0.4f, 0.1f, false, 0.0f));
    }
}

void AudioKit::click() {
    if (!play("ui_click", 0.7f, randomRange(0.98f, 1.02f))) {
        push(makeTone(520.0f, 700.0f, 0.07f, 0.08f, false, 0.0f));
    }
}

void AudioKit::footstep(float speed) {
    const char* clip = (m_footstepIndex++ & 1) ? "footstep_2" : "footstep_1";
    const float gain = std::clamp(0.24f + std::min(0.3f, speed * 0.035f), 0.2f, 0.6f);
    if (!play(clip, gain, randomRange(0.94f, 1.05f), 0.25f)) {
        push(makeImpact("concrete", 0.16f));
    }
}

float AudioKit::noise() {
    return randomRange(-1.0f, 1.0f);
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

} // namespace vd
