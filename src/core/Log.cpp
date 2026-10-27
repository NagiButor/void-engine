#include "void/core/Log.h"

#include <cstdio>
#include <cstring>
#include <fstream>
#include <mutex>
#include <string>

#ifdef _WIN32
#include <windows.h>
#endif

namespace vd::log {
namespace {
std::mutex g_mutex;
std::ofstream g_file;
bool g_initialized = false;

const char* levelName(Level level) {
    switch (level) {
    case Level::Info: return "INFO";
    case Level::Warn: return "WARN";
    case Level::Error: return "ERROR";
    case Level::Debug: return "DEBUG";
    }
    return "?";
}
} // namespace

void init(const std::string& logFile) {
    std::lock_guard lock(g_mutex);
    if (!logFile.empty()) {
        g_file.open(logFile, std::ios::out | std::ios::trunc);
    }
    g_initialized = true;
}

void shutdown() {
    std::lock_guard lock(g_mutex);
    if (g_file.is_open()) {
        g_file.flush();
        g_file.close();
    }
    g_initialized = false;
}

void write(Level level, const char* fmt, ...) {
    char message[4096];
    va_list args;
    va_start(args, fmt);
    std::vsnprintf(message, sizeof(message), fmt, args);
    va_end(args);

    char line[4600];
    std::snprintf(line, sizeof(line), "[VOID %-5s] %s\n", levelName(level), message);

    std::lock_guard lock(g_mutex);
    std::fputs(line, stdout);
    std::fflush(stdout);
    if (g_file.is_open()) {
        g_file << line;
        g_file.flush();
    }
#ifdef _WIN32
    OutputDebugStringA(line);
#endif
}

#define VD_LOG_FN(name, level)                 \
    void name(const char* fmt, ...) {          \
        char message[4096];                    \
        va_list args;                          \
        va_start(args, fmt);                   \
        std::vsnprintf(message, sizeof(message), fmt, args); \
        va_end(args);                          \
        write(level, "%s", message);           \
    }

VD_LOG_FN(info, Level::Info)
VD_LOG_FN(warn, Level::Warn)
VD_LOG_FN(error, Level::Error)

#ifdef NDEBUG
void debug(const char*, ...) {}
#else
VD_LOG_FN(debug, Level::Debug)
#endif

#undef VD_LOG_FN

} // namespace vd::log
