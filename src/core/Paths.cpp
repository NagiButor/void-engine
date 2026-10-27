#include "void/core/Paths.h"

#include <SDL3/SDL.h>

namespace vd {
namespace {
std::filesystem::path g_base;
std::filesystem::path g_assets;
} // namespace

void Paths::initialize() {
    const char* raw = SDL_GetBasePath();
    if (raw) {
        g_base = std::filesystem::path(raw);
        // SDL3 caches this string internally; it must not be freed by the caller.
    } else {
        g_base = std::filesystem::current_path();
    }
    g_assets = g_base / "assets";
}

const std::filesystem::path& Paths::baseDir() {
    return g_base;
}

const std::filesystem::path& Paths::assetDir() {
    return g_assets;
}

std::filesystem::path Paths::asset(const std::filesystem::path& relative) {
    return g_assets / relative;
}

} // namespace vd
