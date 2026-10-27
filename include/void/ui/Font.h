#pragma once

#include "void/core/Types.h"
#include "void/rhi/RHI.h"

#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

namespace vd {

class Font {
public:
    bool load(const std::string& ttfPath, int bakedSize = 48, int atlasWidth = 2048, int atlasHeight = 1024);

    const std::vector<u8>& pixels() const { return m_pixels; }
    int atlasWidth() const { return m_width; }
    int atlasHeight() const { return m_height; }
    int bakedSize() const { return m_bakedSize; }
    glm::vec2 solidUv() const { return m_solidUv; }

    float measure(std::string_view text, float pixelSize) const;
    void draw(rhi::UIList& list, float x, float y, float pixelSize, glm::vec4 color,
              std::string_view text) const;
    void drawCentered(rhi::UIList& list, float centerX, float y, float pixelSize, glm::vec4 color,
                      std::string_view text) const;

private:
    struct Glyph {
        float x0 = 0.0f, y0 = 0.0f, x1 = 0.0f, y1 = 0.0f;
        float xoff = 0.0f, yoff = 0.0f, advance = 0.0f;
    };

    void pushGlyph(rhi::UIList& list, const Glyph& glyph, float penX, float baselineY,
                   float scale, glm::vec4 color) const;
    const Glyph* find(u32 codepoint) const;

    std::vector<u8> m_pixels;
    std::vector<u8> m_ttf;
    std::unordered_map<u32, Glyph> m_glyphs;
    int m_width = 0;
    int m_height = 0;
    int m_bakedSize = 48;
    glm::vec2 m_solidUv{0.0f};
};

} // namespace vd
