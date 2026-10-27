#pragma once

#include <cstdarg>
#include <string>

namespace vd::log {

enum class Level { Info, Warn, Error, Debug };

void init(const std::string& logFile);
void shutdown();
void write(Level level, const char* fmt, ...);
void info(const char* fmt, ...);
void warn(const char* fmt, ...);
void error(const char* fmt, ...);
void debug(const char* fmt, ...);

} // namespace vd::log
