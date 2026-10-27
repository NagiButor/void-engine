#include "void/ui/Font.h"

#include "void/core/Log.h"

#define STB_TRUETYPE_IMPLEMENTATION
#include <stb_truetype.h>

#include <algorithm>
#include <fstream>
#include <vector>

namespace vd {
namespace {
std::vector<u32> decodeUtf8(std::string_view text) {
    std::vector<u32> out;
    for (std::size_t i = 0; i < text.size();) {
        const unsigned char c = static_cast<unsigned char>(text[i]);
        u32 cp = 0;
        int extra = 0;
        if (c < 0x80) {
            cp = c;
        } else if ((c >> 5) == 0x6) {
            cp = c & 0x1F;
            extra = 1;
        } else if ((c >> 4) == 0xE) {
            cp = c & 0x0F;
            extra = 2;
        } else if ((c >> 3) == 0x1E) {
            cp = c & 0x07;
            extra = 3;
        } else {
            ++i;
            continue;
        }
        if (i + extra >= text.size()) break;
        bool valid = true;
        for (int j = 1; j <= extra; ++j) {
            const unsigned char cc = static_cast<unsigned char>(text[i + j]);
            if ((cc & 0xC0) != 0x80) {
                valid = false;
                break;
            }
            cp = (cp << 6) | (cc & 0x3F);
        }
        i += extra + 1;
        if (valid) out.push_back(cp);
    }
    return out;
}
} // namespace

bool Font::load(const std::string& ttfPath, int bakedSize, int atlasWidth, int atlasHeight) {
    std::ifstream file(ttfPath, std::ios::binary | std::ios::ate);
    if (!file) {
        log::error("Font: cannot open %s", ttfPath.c_str());
        return false;
    }
    const std::streamsize size = file.tellg();
    file.seekg(0);
    m_ttf.resize(static_cast<size_t>(size));
    file.read(reinterpret_cast<char*>(m_ttf.data()), size);

    m_bakedSize = bakedSize;
    m_width = atlasWidth;
    m_height = atlasHeight;
    m_pixels.assign(static_cast<size_t>(atlasWidth) * atlasHeight, 0);

    stbtt_pack_context pack{};
    if (!stbtt_PackBegin(&pack, m_pixels.data(), atlasWidth, atlasHeight, 0, 1, nullptr)) {
        log::error("Font: stbtt_PackBegin failed");
        return false;
    }
    stbtt_PackSetOversampling(&pack, 1, 1);

    std::vector<stbtt_packedchar> ascii(95);
    std::vector<stbtt_packedchar> cyrillic(256);
    stbtt_pack_range ranges[2]{};
    ranges[0].font_size = static_cast<float>(bakedSize);
    ranges[0].first_unicode_codepoint_in_range = 32;
    ranges[0].num_chars = 95;
    ranges[0].chardata_for_range = ascii.data();
    ranges[1].font_size = static_cast<float>(bakedSize);
    ranges[1].first_unicode_codepoint_in_range = 0x400;
    ranges[1].num_chars = 256;
    ranges[1].chardata_for_range = cyrillic.data();

    if (!stbtt_PackFontRanges(&pack, m_ttf.data(), 0, ranges, 2)) {
        log::error("Font: stbtt_PackFontRanges failed");
        stbtt_PackEnd(&pack);
        return false;
    }
    stbtt_PackEnd(&pack);

    auto addGlyph = [&](u32 codepoint, const stbtt_packedchar& pc) {
        Glyph glyph;
        glyph.x0 = static_cast<float>(pc.x0);
        glyph.y0 = static_cast<float>(pc.y0);
        glyph.x1 = static_cast<float>(pc.x1);
        glyph.y1 = static_cast<float>(pc.y1);
        glyph.xoff = pc.xoff;
        glyph.yoff = pc.yoff;
        glyph.advance = pc.xadvance;
        m_glyphs[codepoint] = glyph;
    };
    for (int i = 0; i < 95; ++i) addGlyph(32 + i, ascii[i]);
    for (int i = 0; i < 256; ++i) addGlyph(0x400 + i, cyrillic[i]);

    m_pixels[0] = 255;
    m_pixels[1] = 255;
    m_solidUv = {0.5f / atlasWidth, 0.5f / atlasHeight};
    u8 maxAlpha = 0;
    double sum = 0.0;
    for (u8 p : m_pixels) { maxAlpha = std::max(maxAlpha, p); sum += p; }
    log::info("Font loaded: %d glyphs, atlas %dx%d, max alpha %u, avg %.1f", static_cast<int>(m_glyphs.size()), atlasWidth, atlasHeight, maxAlpha, sum / m_pixels.size());
    return true;
}

const Font::Glyph* Font::find(u32 codepoint) const {
    auto it = m_glyphs.find(codepoint);
    return it == m_glyphs.end() ? nullptr : &it->second;
}

float Font::measure(std::string_view text, float pixelSize) const {
    const float scale = pixelSize / static_cast<float>(m_bakedSize);
    float x = 0.0f;
    float lineWidth = 0.0f;
    for (u32 cp : decodeUtf8(text)) {
        if (cp == '\n') {
            lineWidth = std::max(lineWidth, x);
            x = 0.0f;
            continue;
        }
        if (const Glyph* glyph = find(cp)) x += glyph->advance * scale;
        else x += pixelSize * 0.3f;
    }
    return std::max(lineWidth, x);
}

void Font::pushGlyph(rhi::UIList& list, const Glyph& g, float penX, float baselineY,
                     float scale, glm::vec4 color) const {
    const float x0 = penX + g.xoff * scale;
    const float y0 = baselineY + g.yoff * scale;
    const float x1 = x0 + (g.x1 - g.x0) * scale;
    const float y1 = y0 + (g.y1 - g.y0) * scale;
    const float u0 = g.x0 / m_width;
    const float v0 = g.y0 / m_height;
    const float u1 = g.x1 / m_width;
    const float v1 = g.y1 / m_height;
    list.vertices.push_back({{x0, y0}, {u0, v0}, color});
    list.vertices.push_back({{x1, y0}, {u1, v0}, color});
    list.vertices.push_back({{x1, y1}, {u1, v1}, color});
    list.vertices.push_back({{x0, y0}, {u0, v0}, color});
    list.vertices.push_back({{x1, y1}, {u1, v1}, color});
    list.vertices.push_back({{x0, y1}, {u0, v1}, color});
}

void Font::draw(rhi::UIList& list, float x, float y, float pixelSize, glm::vec4 color,
                std::string_view text) const {
    const float scale = pixelSize / static_cast<float>(m_bakedSize);
    float penX = x;
    float baseline = y + pixelSize * 0.78f;
    for (u32 cp : decodeUtf8(text)) {
        if (cp == '\n') {
            penX = x;
            baseline += pixelSize * 1.25f;
            continue;
        }
        if (cp == '\r') continue;
        if (const Glyph* glyph = find(cp)) {
            pushGlyph(list, *glyph, penX, baseline, scale, color);
            penX += glyph->advance * scale;
        } else {
            penX += pixelSize * 0.3f;
        }
    }
}

void Font::drawCentered(rhi::UIList& list, float centerX, float y, float pixelSize, glm::vec4 color,
                        std::string_view text) const {
    draw(list, centerX - measure(text, pixelSize) * 0.5f, y, pixelSize, color, text);
}

} // namespace vd
