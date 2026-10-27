#include "void/render/Textures.h"

#include "void/core/Log.h"

#include <stb_truetype.h>

#include <algorithm>
#include <cmath>
#include <fstream>
#include <random>
#include <string>
#include <vector>

namespace vd::textures {
namespace {
constexpr int kSize = 512;
using Rng = std::mt19937;

float frand(Rng& rng) {
    return static_cast<float>(rng() & 0xFFFFFF) / static_cast<float>(0x1000000);
}

rhi::ImageData makeImage(u32 layer) {
    rhi::ImageData image;
    image.width = kSize;
    image.height = kSize;
    image.srgb = true;
    image.pixels.assign(static_cast<size_t>(kSize) * kSize * 4, 255);
    (void)layer;
    return image;
}

void setPixel(rhi::ImageData& image, int x, int y, float r, float g, float b, float a = 1.0f) {
    if (x < 0 || y < 0 || x >= static_cast<int>(image.width) || y >= static_cast<int>(image.height)) return;
    u8* p = &image.pixels[(static_cast<size_t>(y) * image.width + static_cast<size_t>(x)) * 4];
    const float inv = 1.0f - a;
    p[0] = static_cast<u8>(std::clamp(p[0] * inv + r * a, 0.0f, 255.0f));
    p[1] = static_cast<u8>(std::clamp(p[1] * inv + g * a, 0.0f, 255.0f));
    p[2] = static_cast<u8>(std::clamp(p[2] * inv + b * a, 0.0f, 255.0f));
    p[3] = 255;
}

void fill(rhi::ImageData& image, int r, int g, int b) {
    for (int y = 0; y < static_cast<int>(image.height); ++y) {
        for (int x = 0; x < static_cast<int>(image.width); ++x) {
            setPixel(image, x, y, static_cast<float>(r), static_cast<float>(g), static_cast<float>(b));
        }
    }
}

void grain(rhi::ImageData& image, Rng& rng, int amount) {
    for (int y = 0; y < static_cast<int>(image.height); ++y) {
        for (int x = 0; x < static_cast<int>(image.width); ++x) {
            const int n = static_cast<int>(frand(rng) * amount * 2.0f) - amount;
            u8* p = &image.pixels[(static_cast<size_t>(y) * image.width + static_cast<size_t>(x)) * 4];
            p[0] = static_cast<u8>(std::clamp(static_cast<int>(p[0]) + n, 0, 255));
            p[1] = static_cast<u8>(std::clamp(static_cast<int>(p[1]) + n, 0, 255));
            p[2] = static_cast<u8>(std::clamp(static_cast<int>(p[2]) + n, 0, 255));
        }
    }
}

void rect(rhi::ImageData& image, int x, int y, int w, int h, int r, int g, int b, float a = 1.0f) {
    for (int yy = y; yy < y + h; ++yy) {
        for (int xx = x; xx < x + w; ++xx) {
            setPixel(image, xx, yy, static_cast<float>(r), static_cast<float>(g), static_cast<float>(b), a);
        }
    }
}

void line(rhi::ImageData& image, int x0, int y0, int x1, int y1, int r, int g, int b, float a = 1.0f) {
    const int dx = std::abs(x1 - x0);
    const int dy = -std::abs(y1 - y0);
    const int sx = x0 < x1 ? 1 : -1;
    const int sy = y0 < y1 ? 1 : -1;
    int err = dx + dy;
    while (true) {
        setPixel(image, x0, y0, static_cast<float>(r), static_cast<float>(g), static_cast<float>(b), a);
        if (x0 == x1 && y0 == y1) break;
        const int e2 = 2 * err;
        if (e2 >= dy) {
            err += dy;
            x0 += sx;
        }
        if (e2 <= dx) {
            err += dx;
            y0 += sy;
        }
    }
}

std::vector<u8> loadTtf(const std::string& path) {
    std::ifstream file(path, std::ios::binary | std::ios::ate);
    if (!file) return {};
    const std::streamsize size = file.tellg();
    file.seekg(0);
    std::vector<u8> data(static_cast<size_t>(size));
    file.read(reinterpret_cast<char*>(data.data()), size);
    return data;
}

void drawText(rhi::ImageData& image, const std::vector<u8>& ttf, const char* text, float pixelSize,
              int x, int y, int r, int g, int b) {
    if (ttf.empty()) return;
    stbtt_fontinfo info{};
    if (!stbtt_InitFont(&info, ttf.data(), 0)) return;
    float scale = stbtt_ScaleForPixelHeight(&info, pixelSize);
    int ascent = 0;
    int descent = 0;
    int lineGap = 0;
    stbtt_GetFontVMetrics(&info, &ascent, &descent, &lineGap);
    const int baseline = y + static_cast<int>(ascent * scale);
    int penX = x;
    for (const char* c = text; *c; ++c) {
        const int cp = static_cast<unsigned char>(*c);
        if (cp == '\n') {
            penX = x;
            continue;
        }
        int advance = 0;
        int lsb = 0;
        stbtt_GetCodepointHMetrics(&info, cp, &advance, &lsb);
        int w = 0;
        int h = 0;
        int xoff = 0;
        int yoff = 0;
        unsigned char* bitmap = stbtt_GetCodepointBitmap(&info, scale, scale, cp, &w, &h, &xoff, &yoff);
        if (bitmap) {
            for (int yy = 0; yy < h; ++yy) {
                for (int xx = 0; xx < w; ++xx) {
                    const float a = bitmap[yy * w + xx] / 255.0f;
                    setPixel(image, penX + xoff + xx, baseline + yoff + yy,
                             static_cast<float>(r), static_cast<float>(g), static_cast<float>(b), a);
                }
            }
            stbtt_FreeBitmap(bitmap, nullptr);
        }
        penX += static_cast<int>(advance * scale);
    }
}

rhi::ImageData makeWhite() {
    auto image = makeImage(Layer::White);
    fill(image, 255, 255, 255);
    return image;
}

rhi::ImageData makeWood(Rng& rng) {
    auto image = makeImage(Layer::Wood);
    fill(image, 168, 118, 68);
    const int plank = 76;
    for (int y = 0; y < kSize; y += plank) {
        const int shade = 120 + static_cast<int>(frand(rng) * 35);
        rect(image, 0, y + 2, kSize, plank - 4, shade + 25, shade - 15, shade - 62);
        line(image, 0, y, kSize, y, 55, 34, 18, 0.6f);
        for (int i = 0; i < 7; ++i) {
            const int gy = y + 8 + static_cast<int>(frand(rng) * (plank - 16));
            line(image, 0, gy, kSize, gy + static_cast<int>(frand(rng) * 5) - 2, 80, 48, 20, 0.15f);
        }
    }
    grain(image, rng, 3);
    return image;
}

rhi::ImageData makeCrate(const std::vector<u8>& ttf, Rng& rng) {
    auto image = makeImage(Layer::Crate);
    fill(image, 142, 94, 50);
    for (int y = 0; y < kSize; y += 72) {
        const int shade = 118 + static_cast<int>(frand(rng) * 35);
        rect(image, 0, y + 2, kSize, 68, shade + 35, shade - 4, shade - 55);
        line(image, 0, y, kSize, y, 56, 32, 14, 0.5f);
        for (int i = 0; i < 9; ++i) {
            const int gy = y + 7 + static_cast<int>(frand(rng) * 58);
            line(image, 0, gy, kSize, gy + static_cast<int>(frand(rng) * 4) - 2, 70, 40, 16, 0.13f);
        }
    }
    const int frame = 48;
    rect(image, 0, 0, kSize, frame, 84, 52, 28);
    rect(image, 0, kSize - frame, kSize, frame, 84, 52, 28);
    rect(image, 0, 0, frame, kSize, 84, 52, 28);
    rect(image, kSize - frame, 0, frame, kSize, 84, 52, 28);
    rect(image, 3, 3, kSize - 6, 3, 38, 22, 10, 0.6f);
    rect(image, 3, kSize - 6, kSize - 6, 3, 38, 22, 10, 0.6f);
    drawText(image, ttf, "VOID", 64.0f, 122, 182, 232, 220, 190);
    drawText(image, ttf, "HANDLE WITH FORCE", 17.0f, 122, 262, 232, 220, 190);
    for (int y = 22; y < kSize; y += 150) {
        for (int x = 22; x < kSize; x += 150) {
            setPixel(image, x, y, 35, 24, 14);
            setPixel(image, x + 1, y, 35, 24, 14);
            setPixel(image, x, y + 1, 35, 24, 14);
        }
    }
    grain(image, rng, 6);
    return image;
}

rhi::ImageData makeConcrete(Rng& rng) {
    auto image = makeImage(Layer::Concrete);
    fill(image, 128, 126, 118);
    for (int y = 0; y < kSize; y += 128) {
        for (int x = 0; x < kSize; x += 128) {
            const int shade = 118 + static_cast<int>(frand(rng) * 24);
            rect(image, x + 1, y + 1, 126, 126, shade, shade - 1, shade - 5);
        }
        line(image, 0, y, kSize, y, 55, 54, 50, 0.4f);
    }
    for (int x = 0; x < kSize; x += 128) line(image, x, 0, x, kSize, 55, 54, 50, 0.4f);
    for (int i = 0; i < 45; ++i) {
        const int x = static_cast<int>(frand(rng) * kSize);
        const int y = static_cast<int>(frand(rng) * kSize);
        const int rr = 8 + static_cast<int>(frand(rng) * 30);
        for (int yy = -rr; yy <= rr; ++yy) {
            for (int xx = -rr; xx <= rr; ++xx) {
                if (xx * xx + yy * yy > rr * rr) continue;
                setPixel(image, x + xx, y + yy, 55, 50, 40, 0.02f);
            }
        }
    }
    grain(image, rng, 9);
    return image;
}

rhi::ImageData makeMetal(Rng& rng) {
    auto image = makeImage(Layer::Metal);
    fill(image, 78, 88, 94);
    for (int x = 0; x < kSize; ++x) {
        const int v = static_cast<int>(frand(rng) * 16) - 8;
        rect(image, x, 0, 1, kSize, 96 + v, 104 + v, 108 + v, 0.12f);
    }
    for (int x = 0; x < kSize; x += 128) {
        rect(image, x, 0, 3, kSize, 35, 38, 40, 0.45f);
        rect(image, x + 3, 0, 2, kSize, 180, 188, 190, 0.12f);
    }
    for (int y = 32; y < kSize; y += 64) {
        for (int x = 18; x < kSize; x += 64) {
            setPixel(image, x, y, 180, 188, 190);
            setPixel(image, x + 1, y, 45, 48, 50);
            setPixel(image, x, y + 1, 45, 48, 50);
        }
    }
    for (int i = 0; i < 12; ++i) {
        const int x = static_cast<int>(frand(rng) * kSize);
        line(image, x, static_cast<int>(frand(rng) * 100), x + 2, 100 + static_cast<int>(frand(rng) * 200),
             110, 62, 30, 0.2f);
    }
    grain(image, rng, 5);
    return image;
}

rhi::ImageData makeHazard(Rng& rng) {
    auto image = makeImage(Layer::Hazard);
    fill(image, 210, 157, 26);
    for (int x = -kSize; x < kSize * 2; x += 64) {
        for (int y = 0; y < kSize; ++y) {
            for (int w = 0; w < 32; ++w) {
                setPixel(image, x + y + w, y, 30, 30, 27, 0.95f);
            }
        }
    }
    grain(image, rng, 7);
    return image;
}

rhi::ImageData makeBarrel(const std::vector<u8>& ttf, Rng& rng) {
    auto image = makeImage(Layer::Barrel);
    fill(image, 55, 105, 130);
    for (int y : {100, 260, 420}) {
        rect(image, 0, y - 14, kSize, 28, 20, 28, 32, 0.35f);
        rect(image, 0, y - 18, kSize, 5, 255, 255, 255, 0.12f);
    }
    rect(image, 0, 176, kSize, 30, 215, 166, 52);
    rect(image, 0, 176, kSize, 4, 30, 30, 24, 0.3f);
    drawText(image, ttf, "VOID", 52.0f, 176, 200, 240, 226, 196);
    drawText(image, ttf, "INDUSTRIAL", 20.0f, 180, 260, 240, 226, 196);
    for (int i = 0; i < 26; ++i) {
        const int x = static_cast<int>(frand(rng) * kSize);
        line(image, x, static_cast<int>(frand(rng) * 200), x + 2, 300 + static_cast<int>(frand(rng) * 200),
             70, 38, 20, 0.14f);
    }
    grain(image, rng, 5);
    return image;
}

rhi::ImageData makeSign(const std::vector<u8>& ttf, Rng& rng) {
    auto image = makeImage(Layer::Sign);
    fill(image, 25, 29, 28);
    rect(image, 12, 12, kSize - 24, 4, 198, 138, 44);
    rect(image, 12, kSize - 16, kSize - 24, 4, 198, 138, 44);
    rect(image, 12, 12, 4, kSize - 24, 198, 138, 44);
    rect(image, kSize - 16, 12, 4, kSize - 24, 198, 138, 44);
    drawText(image, ttf, "VOID", 72.0f, 38, 52, 236, 222, 198);
    drawText(image, ttf, "ENGINE", 72.0f, 38, 130, 214, 151, 58);
    drawText(image, ttf, "SECTOR 07", 26.0f, 40, 238, 236, 222, 198);
    drawText(image, ttf, "PHYSICS TEST RANGE", 18.0f, 40, 280, 150, 144, 132);
    drawText(image, ttf, "AUTHORIZED PERSONNEL ONLY", 15.0f, 40, 310, 110, 106, 98);
    for (int i = 0; i < 30; ++i) {
        int x = 380 + i * 4;
        int h = 20 + static_cast<int>(frand(rng) * 60);
        rect(image, x, 380, 2, h, 210, 200, 178, 0.45f);
    }
    grain(image, rng, 4);
    return image;
}
} // namespace

std::vector<rhi::ImageData> createLabTextures(const std::string& fontPath) {
    Rng rng(0x51A7u);
    const std::vector<u8> ttf = loadTtf(fontPath);

    std::vector<rhi::ImageData> layers;
    layers.reserve(LayerCount);
    layers.push_back(makeWhite());
    layers.push_back(makeCrate(ttf, rng));
    layers.push_back(makeWood(rng));
    layers.push_back(makeConcrete(rng));
    layers.push_back(makeMetal(rng));
    layers.push_back(makeHazard(rng));
    layers.push_back(makeBarrel(ttf, rng));
    layers.push_back(makeSign(ttf, rng));
    log::info("Generated %d procedural material layers", static_cast<int>(layers.size()));
    return layers;
}

} // namespace vd::textures
