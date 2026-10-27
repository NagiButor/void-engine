#pragma once

#include <filesystem>

namespace vd {

class Paths {
public:
    static void initialize();
    static const std::filesystem::path& baseDir();
    static const std::filesystem::path& assetDir();
    static std::filesystem::path asset(const std::filesystem::path& relative);
};

} // namespace vd
