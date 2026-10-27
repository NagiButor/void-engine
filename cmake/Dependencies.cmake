include(FetchContent)

set(FETCHCONTENT_QUIET OFF)
set(FETCHCONTENT_BASE_DIR "${CMAKE_BINARY_DIR}/_deps" CACHE PATH "" FORCE)

# ---------------------------------------------------------------------------
# SDL3 - window, input, platform, audio device
# ---------------------------------------------------------------------------
set(SDL_SHARED OFF CACHE BOOL "" FORCE)
set(SDL_STATIC ON CACHE BOOL "" FORCE)
set(SDL_TEST_LIBRARY OFF CACHE BOOL "" FORCE)
set(SDL_TESTS OFF CACHE BOOL "" FORCE)
set(SDL_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SDL_INSTALL OFF CACHE BOOL "" FORCE)
set(SDL_INSTALL_TESTS OFF CACHE BOOL "" FORCE)
set(SDL_DISKAUDIO ON CACHE BOOL "" FORCE)
set(SDL_DUMMYAUDIO ON CACHE BOOL "" FORCE)

FetchContent_Declare(SDL3
    GIT_REPOSITORY https://github.com/libsdl-org/SDL.git
    GIT_TAG release-3.4.18
    GIT_SHALLOW TRUE
)
FetchContent_MakeAvailable(SDL3)

# ---------------------------------------------------------------------------
# GLM - math. Wrapped by vd::math so it can be replaced if needed.
# ---------------------------------------------------------------------------
set(GLM_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(GLM_BUILD_INSTALL OFF CACHE BOOL "" FORCE)
FetchContent_Declare(glm
    GIT_REPOSITORY https://github.com/g-truc/glm.git
    GIT_TAG 1.0.3
    GIT_SHALLOW TRUE
)
FetchContent_MakeAvailable(glm)

# ---------------------------------------------------------------------------
# Box3D - native 3D rigid body physics by Erin Catto (MIT)
# ---------------------------------------------------------------------------
set(BOX3D_SAMPLES OFF CACHE BOOL "" FORCE)
set(BOX3D_BENCHMARKS OFF CACHE BOOL "" FORCE)
set(BOX3D_DOCS OFF CACHE BOOL "" FORCE)
set(BOX3D_UNIT_TESTS OFF CACHE BOOL "" FORCE)
set(BOX3D_PROFILE OFF CACHE BOOL "" FORCE)
set(BOX3D_VALIDATE OFF CACHE BOOL "" FORCE)
set(BOX3D_COMPILE_WARNING_AS_ERROR OFF CACHE BOOL "" FORCE)
FetchContent_Declare(box3d
    GIT_REPOSITORY https://github.com/erincatto/box3d.git
    GIT_TAG v0.1.0
    GIT_SHALLOW TRUE
)
FetchContent_MakeAvailable(box3d)

# Box3D changes global output directories as a convenience for its own
# samples. Restore ours after it has been configured.
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin")
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/lib")
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/lib")

# ---------------------------------------------------------------------------
# Vulkan headers + volk. No Vulkan SDK is required: volk resolves the
# system vulkan-1.dll at runtime.
# ---------------------------------------------------------------------------
if(VOID_BUILD_VULKAN)
    set(VULKAN_HEADERS_ENABLE_TESTS OFF CACHE BOOL "" FORCE)
    set(VULKAN_HEADERS_ENABLE_MODULE OFF CACHE BOOL "" FORCE)
    set(VULKAN_HEADERS_ENABLE_INSTALL OFF CACHE BOOL "" FORCE)
    FetchContent_Declare(VulkanHeaders
        GIT_REPOSITORY https://github.com/KhronosGroup/Vulkan-Headers.git
        GIT_TAG vulkan-sdk-1.4.363.0
        GIT_SHALLOW TRUE
    )
    FetchContent_MakeAvailable(VulkanHeaders)

    set(VOLK_PULL_IN_VULKAN OFF CACHE BOOL "" FORCE)
    set(VOLK_INSTALL OFF CACHE BOOL "" FORCE)
    FetchContent_Declare(volk
        GIT_REPOSITORY https://github.com/zeux/volk.git
        GIT_TAG vulkan-sdk-1.4.363.0
        GIT_SHALLOW TRUE
    )
    FetchContent_MakeAvailable(volk)

    # volk.h needs the same headers.
    target_link_libraries(volk PUBLIC Vulkan::Headers)
endif()

# ---------------------------------------------------------------------------
# stb_truetype - font atlas rasterization (MIT)
# ---------------------------------------------------------------------------
FetchContent_Declare(stb
    GIT_REPOSITORY https://github.com/nothings/stb.git
    GIT_TAG 003501f6d7a8668ce8b6d875e34dd97ad0d5d5b5
    GIT_SHALLOW TRUE
)
FetchContent_MakeAvailable(stb)
