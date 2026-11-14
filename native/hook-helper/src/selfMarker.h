#pragma once
// user's: the wheel's injected keys (hooks_win.cpp) and the context helper's paste (context_write_win.cpp). macOS puts
// the same value in kCGEventSourceUserData (hooks_mac.cpp, and context_write_mac.mm for the paste).
#include <cstdint>

namespace lp {

/** "LPIN" in an injected event's dwExtraInfo. */
constexpr std::uintptr_t kSelfMarker = 0x4C50494E;

}  // namespace lp
