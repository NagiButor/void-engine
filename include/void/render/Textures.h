#pragma once

#include "void/rhi/RHI.h"

#include <vector>

namespace vd::textures {

enum Layer : u32 {
    White = 0,
    Crate = 1,
    Wood = 2,
    Concrete = 3,
    Metal = 4,
    Hazard = 5,
    Barrel = 6,
    Sign = 7,
    LayerCount = 8,
};

std::vector<rhi::ImageData> createLabTextures(const std::string& fontPath);

} // namespace vd::textures
