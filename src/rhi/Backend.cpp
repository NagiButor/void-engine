#include "void/rhi/RHI.h"

namespace vd::rhi {

const char* apiName(Api api) {
    switch (api) {
    case Api::DX12: return "DirectX 12";
    case Api::Vulkan: return "Vulkan";
    case Api::Auto: return "Auto";
    }
    return "Unknown";
}

} // namespace vd::rhi
