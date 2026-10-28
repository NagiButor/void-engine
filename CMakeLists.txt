cmake_minimum_required(VERSION 3.24)

project(VoidEngine
    VERSION 0.1.0
    DESCRIPTION "V.O.I.D. Engine - Virtual Objects & Interactive Dynamics"
    HOMEPAGE_URL "https://github.com/NagiButor/void-engine"
    LANGUAGES C CXX
)

set(CMAKE_C_STANDARD 17)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Box3D, SDL3 and the engine all use the static CRT so the final demo is
# self-contained and no CRT boundary is crossed between static libraries.
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>" CACHE STRING "" FORCE)

set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin")
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/lib")
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/lib")
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

option(VOID_BUILD_DX12 "Build the DirectX 12 backend" ON)
option(VOID_BUILD_VULKAN "Build the Vulkan backend" ON)
option(VOID_WARNINGS_AS_ERRORS "Treat engine warnings as errors" OFF)

if(MSVC)
    add_compile_options(/utf-8 /permissive- /W4 /MP)
    if(VOID_WARNINGS_AS_ERRORS)
        add_compile_options(/WX)
    endif()
    add_compile_definitions(_CRT_SECURE_NO_WARNINGS NOMINMAX WIN32_LEAN_AND_MEAN)
endif()

include(cmake/Dependencies.cmake)
include(cmake/Shaders.cmake)

set(VOID_PUBLIC_INCLUDE "${CMAKE_CURRENT_SOURCE_DIR}/include")
set(VOID_SOURCE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/src")

add_library(void_engine STATIC
    src/core/Log.cpp
    src/core/Paths.cpp
    src/platform/Window.cpp
    src/platform/Input.cpp
    src/rhi/Backend.cpp
    src/render/MeshFactory.cpp
    src/render/Textures.cpp
    src/render/GLTFLoader.cpp
    src/physics/PhysicsWorld.cpp
    src/ui/Font.cpp
    src/audio/Audio.cpp
)

target_include_directories(void_engine
    PUBLIC ${VOID_PUBLIC_INCLUDE}
    PRIVATE ${VOID_SOURCE_DIR} "${stb_SOURCE_DIR}"
)
target_compile_definitions(void_engine PUBLIC SDL_MAIN_HANDLED)

target_link_libraries(void_engine
    PUBLIC
        glm::glm
        box3d::box3d
        SDL3::SDL3-static
)
target_link_libraries(void_engine PRIVATE nlohmann_json::nlohmann_json)

if(VOID_BUILD_DX12)
    target_sources(void_engine PRIVATE src/rhi/dx12/DX12Device.cpp)
    target_link_libraries(void_engine PRIVATE d3d12 dxgi dxguid)
    target_compile_definitions(void_engine PUBLIC VOID_WITH_DX12=1)
endif()

if(VOID_BUILD_VULKAN)
    target_sources(void_engine PRIVATE src/rhi/vulkan/VulkanDevice.cpp)
    target_link_libraries(void_engine PRIVATE volk::volk Vulkan::Headers)
    target_compile_definitions(void_engine PUBLIC VOID_WITH_VULKAN=1)
endif()

add_executable(void_demo
    src/main.cpp
    src/demo/Demo.cpp
)
target_include_directories(void_demo PRIVATE ${VOID_SOURCE_DIR})
target_link_libraries(void_demo PRIVATE void_engine)
add_dependencies(void_demo void_shaders)

set_target_properties(void_demo PROPERTIES
    OUTPUT_NAME "void_demo"
    WIN32_EXECUTABLE TRUE
    VS_DEBUGGER_WORKING_DIRECTORY "$<TARGET_FILE_DIR:void_demo>"
)
if(MSVC)
    # Keep the normal main() entry point while linking as a GUI application.
    target_link_options(void_demo PRIVATE /ENTRY:mainCRTStartup)
endif()

# Assets: compiled shaders land in the build tree, fonts come from source.
set(VOID_ASSET_OUT "${CMAKE_BINARY_DIR}/assets")
add_custom_command(TARGET void_demo POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E make_directory "$<TARGET_FILE_DIR:void_demo>/assets"
    COMMAND ${CMAKE_COMMAND} -E copy_directory "${VOID_ASSET_OUT}" "$<TARGET_FILE_DIR:void_demo>/assets"
    COMMAND ${CMAKE_COMMAND} -E copy_directory "${CMAKE_CURRENT_SOURCE_DIR}/assets/fonts" "$<TARGET_FILE_DIR:void_demo>/assets/fonts"
    COMMAND ${CMAKE_COMMAND} -E copy_directory "${CMAKE_CURRENT_SOURCE_DIR}/assets/audio" "$<TARGET_FILE_DIR:void_demo>/assets/audio"
    COMMAND ${CMAKE_COMMAND} -E copy_directory "${CMAKE_CURRENT_SOURCE_DIR}/assets/models" "$<TARGET_FILE_DIR:void_demo>/assets/models"
    COMMENT "Copying V.O.I.D. assets"
)

# A small install/export surface so the engine can later be licensed and
# consumed by other projects without dragging the demo along.
install(TARGETS void_engine
    ARCHIVE DESTINATION lib
    RUNTIME DESTINATION bin
)
install(DIRECTORY include/void DESTINATION include)
