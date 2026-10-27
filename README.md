# V.O.I.D. Engine
**Virtual Objects & Interactive Dynamics**

![Language](https://shields.io)
![Graphics](https://shields.io)
![Physics](https://shields.io)
![Build](https://shields.io)
![Status](https://shields.io)

**VOID** is a custom, high-performance 3D game engine written from scratch in modern C++20. Built with a focus on low-level hardware control, data-oriented architecture, and massive physical interactivity, VOID is designed as a proprietary foundation for next-generation games.

---

## ⚡ Key Features

### 🖥️ Rendering (RHI)
* **Dual Low-Level Backend:** Native Render Hardware Interface (RHI) built specifically for **DirectX 12** and **Vulkan**.
* **Zero-Overhead Abstraction:** Direct control over GPU memory, command queues, and pipeline state objects (PSO).
* **Multithreaded Command Recording:** Designed to saturate modern multi-core CPUs during scene submission.

### 🌐 Interactive Dynamics (Physics)
* **Powered by [Box3D](https://github.com):** Integrates Erin Catto's cutting-edge 3D physics engine written in portable C17.
* **Robust Simulation:** Features a *Soft Step* rigid body solver, continuous collision detection (CCD), and island-based sleeping.
* **Hardware Accelerated:** Heavily utilizes multithreading and SIMD instructions (AVX2, SSE2, Neon) for simulating massive piles of dynamic objects.

### ⚙️ Core Architecture
* **Data-Oriented Design (DOD):** Cache-friendly memory layouts and flat data structures for maximum execution speed.
* **Modern C++20 / C17:** Clean, orthodox codebase avoiding heavy OOP overhead in performance-critical paths.
* **Deterministic & Modular:** Strictly separated engine core (`vd::`) and game logic layers.

---

## 🛠️ Requirements & Dependencies

To build **VOID Engine**, ensure you have the following installed:
* **Compiler:** C++20 and C17 compatible compiler (MSVC 2022+, Clang 15+, or GCC 12+)
* **Build System:** CMake 3.24+
* **Graphics SDKs:** 
  * Windows 10/11 SDK (for DirectX 12)
  * Vulkan SDK (for Vulkan API)
* **Git** (used by CMake `FetchContent` to automatically pull dependencies)

### Third-Party Libraries
* **[Box3D](https://github.com)** — 3D rigid body physics engine (fetched automatically via CMake).

---

## 🚀 Building the Engine

Clone the repository and use CMake to configure and build the project:

```bash
git clone https://github.com
cd void-engine
mkdir build && cd build
cmake .. -DCMAKE_BUILD_TYPE=Release
cmake --build . --config Release
```

---

## 📂 Project Structure

```text
void-engine/
├── CMakeLists.txt          # Root CMake configuration
├── README.md               # Engine documentation
├── extern/                 # Third-party dependencies & submodules
├── include/
│   └── void/               # Public engine headers (vd:: namespace)
├── assets/                 # Shaders (HLSL/SPIR-V), meshes, textures
└── src/
    ├── core/               # Memory, threading, logging, platform layer
    ├── rhi/                # Render Hardware Interface
    │   ├── dx12/           # DirectX 12 backend
    │   └── vulkan/         # Vulkan backend
    ├── physics/            # Box3D integration & dynamics wrapper
    └── scene/              # Virtual objects, ECS, and world management
```

---

## 👤 Author & Credits

* **Lead Engine Developer:** **Mi5hlen**
* **Physics Engine:** [Box3D](https://github.com) by Erin Catto (MIT License)

---
*V.O.I.D. Engine — Virtual Objects & Interactive Dynamics.*
