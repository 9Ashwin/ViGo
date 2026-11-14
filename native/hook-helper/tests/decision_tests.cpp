// Unit tests for the hook helper's gesture logic (no OS hooks): the launcher's open-on-press wheel, its
// move/release reporting, cancel paths and the unchanged ping-wheel behaviour the shared Decision keeps.
#include <cstdio>
#include <string>

#include "../src/commands.h"
#include "../src/decision.h"
#include "../src/json.h"
#include "../src/macinput.h"

using namespace lp;

static int g_pass = 0;
static int g_fail = 0;
#define CHECK(cond)                                               \
  do {                                                            \
    if (cond) {                                                   \
      ++g_pass;                                                   \
    } else {                                                      \
      ++g_fail;                                                   \
      std::printf("FAIL %s:%d  %s\n", __FILE__, __LINE__, #cond); \
    }                                                             \
  } while (0)

namespace {

using S = Decision::State;

InputEvent mouse(EvKind k, Btn b, int x, int y, uint64_t t = 0) {
  InputEvent e;
  e.kind = k;
  e.btn = b;
  e.x = x;
  e.y = y;
  e.timeMs = t;
  return e;
}
InputEvent md(Btn b, int x, int y, uint64_t t = 0) { return mouse(EvKind::MouseDown, b, x, y, t); }
InputEvent mu(Btn b, int x, int y, uint64_t t = 0) { return mouse(EvKind::MouseUp, b, x, y, t); }
InputEvent mv(int x, int y, uint64_t t = 0) { return mouse(EvKind::MouseMove, Btn::None, x, y, t); }
InputEvent kd(uint32_t vk, uint64_t t = 0) {
  InputEvent e;
  e.kind = EvKind::KeyDown;
  e.vk = vk;
  e.timeMs = t;
  return e;
}
int countEmits(const Result& r, Emit::Kind k) {
  int n = 0;
  for (const auto& e : r.emits) n += e.kind == k ? 1 : 0;
  return n;
}

const Emit* findEmit(const Result& r, Emit::Kind k) {
  for (const auto& e : r.emits) {
    if (e.kind == k) return &e;
  }
  return nullptr;
}

Decision launcher(Trigger t = Trigger::Mouse4) {
  Config c;
  c.trigger = t;
  c.openOnPress = true;
  c.dragThresholdPx = 8;
  Decision d;
  d.setConfig(c);
  return d;
}

// The side-button press opens the wheel right where the pointer is, with no drag at all.
void testOpenOnPress() {
  Decision d = launcher();
  Result r = d.onEvent(md(Btn::X1, 400, 300, 1000));
  CHECK(r.swallow);
  CHECK(d.state() == S::Wheel);
  const Emit* open = findEmit(r, Emit::Kind::WheelOpen);
  CHECK(open != nullptr);
  CHECK(open && open->x == 400 && open->y == 300);

  // A move before any release is a hover, not an open.
  r = d.onEvent(mv(500, 300, 1010));
  CHECK(findEmit(r, Emit::Kind::WheelMove) != nullptr);
  CHECK(countEmits(r, Emit::Kind::WheelOpen) == 0);

  // The release lands on the slice under the pointer and ends the gesture.
  r = d.onEvent(mu(Btn::X1, 520, 310, 1020));
  CHECK(r.swallow);
  const Emit* rel = findEmit(r, Emit::Kind::WheelRelease);
  CHECK(rel != nullptr);
  CHECK(rel && rel->x == 520 && rel->y == 310);
  CHECK(d.state() == S::Idle);
}

// A press-and-release with no movement still opens and releases: the launcher never turns it into a click.
void testOpenThenImmediateRelease() {
  Decision d = launcher();
  d.onEvent(md(Btn::X1, 10, 20, 0));
  Result r = d.onEvent(mu(Btn::X1, 10, 20, 10));
  CHECK(countEmits(r, Emit::Kind::WheelRelease) == 1);
  CHECK(r.injects.empty());  // nothing replayed: the side button is not a real click
  CHECK(d.state() == S::Idle);
}

// The ping-wheel behaviour is untouched: a press alone opens nothing, it waits for the drag.
void testDragThresholdWithoutOpenOnPress() {
  Config c;
  c.trigger = Trigger::Mouse4;
  c.openOnPress = false;
  Decision d;
  d.setConfig(c);
  Result r = d.onEvent(md(Btn::X1, 100, 100, 0));
  CHECK(r.swallow);
  CHECK(d.state() == S::Pending);
  CHECK(countEmits(r, Emit::Kind::WheelOpen) == 0);
  r = d.onEvent(mv(120, 100, 10));  // past the 8 px threshold
  CHECK(countEmits(r, Emit::Kind::WheelOpen) == 1);
  CHECK(d.state() == S::Wheel);
}

// A right-click while the wheel is open cancels it and is swallowed, so the app underneath sees nothing.
void testCancelByRightClick() {
  Decision d = launcher();
  d.onEvent(md(Btn::X1, 50, 50, 0));
  Result r = d.onEvent(md(Btn::Right, 60, 60, 5));
  CHECK(r.swallow);
  CHECK(countEmits(r, Emit::Kind::Cancel) == 1);
  r = d.onEvent(mu(Btn::Right, 60, 60, 6));
  CHECK(r.swallow);  // the matching up of a swallowed press stays swallowed
  CHECK(d.state() == S::SwallowUp);
}

// Losing the button-up (secure input, a locked screen) must not leave the wheel stuck forever.
void testWatchdog() {
  Decision d = launcher();
  d.onEvent(md(Btn::X1, 50, 50, 1000));
  Result r = d.tick(1000 + kWatchdogMs + 1);
  CHECK(countEmits(r, Emit::Kind::Cancel) == 1);
  CHECK(d.state() == S::SwallowUp);
}

// Disabling the app mid-gesture tears the wheel down.
void testSetEnabledCancels() {
  Decision d = launcher();
  d.onEvent(md(Btn::X1, 50, 50, 0));
  Result r = d.setEnabled(false);
  CHECK(countEmits(r, Emit::Kind::Cancel) == 1);
  CHECK(d.state() == S::SwallowUp);
  // While disabled, the same press passes through untouched.
  d = launcher();
  d.setConfig([]{ Config c; c.trigger = Trigger::Mouse4; c.openOnPress = true; c.enabled = false; return c; }());
  r = d.onEvent(md(Btn::X1, 50, 50, 0));
  CHECK(!r.swallow);
  CHECK(r.emits.empty());
}

// A keyboard trigger (⌥) keeps the ping-wheel contract: hold the key and click to open.
void testKeyTriggerStillNeedsClick() {
  Config c;
  c.trigger = Trigger::Alt;
  c.openOnPress = true;  // ignored for a key trigger
  Decision d;
  d.setConfig(c);
  Result r = d.onEvent(kd(0xA4, 0));
  CHECK(!r.swallow);
  r = d.onEvent(md(Btn::Left, 300, 300, 10));
  CHECK(r.swallow);
  CHECK(countEmits(r, Emit::Kind::WheelOpen) == 0);  // Pending until the drag
  CHECK(d.state() == S::Pending);
}

// The config line is what Electron actually sends over stdin.
void testConfigCommand() {
  Config cur;
  std::map<std::string, std::string> fields;
  std::string text = R"({"type":"config","trigger":"mouse5","openOnPress":true,"dragThresholdPx":8,"enabled":true})";
  const Command cmd = parseCommand(text, cur);
  CHECK(cmd.kind == Command::Kind::Config);
  CHECK(cmd.config.trigger == Trigger::Mouse5);
  CHECK(cmd.config.openOnPress);

  // Missing fields keep the current value, so an old Electron cannot reset the flag by omission.
  Config withFlag;
  withFlag.openOnPress = true;
  const Command again = parseCommand(R"({"type":"config","trigger":"mouse4"})", withFlag);
  CHECK(again.config.openOnPress);
}

// macOS key codes ↔ the Windows VKs the protocol speaks.
void testMacInputMapping() {
  CHECK(mac::keyCodeToVk(0x00) == 0x41);   // kVK_ANSI_A
  CHECK(mac::keyCodeToVk(0x35) == 0x1B);   // kVK_Escape
  CHECK(mac::keyCodeToVk(0x37) == 0x5B);   // kVK_Command
  CHECK(mac::vkToKeyCode(0x41) == 0x00);
  CHECK(mac::vkToKeyCode(0x1B) == 0x35);
}

void testHoldVLauncher() {
  Config c;
  c.trigger = Trigger::CustomVk;
  c.triggerVk = 0x56;
  c.openOnPress = true;
  Decision d;
  d.setConfig(c);
  auto down = kd(0x56, 1000);
  down.x = 400; down.y = 300;
  CHECK(d.onEvent(down).swallow);
  CHECK(countEmits(d.tick(1149), Emit::Kind::WheelOpen) == 0);
  CHECK(countEmits(d.tick(1150), Emit::Kind::WheelOpen) == 1);
  CHECK(countEmits(d.onEvent(mv(450, 350, 1160)), Emit::Kind::WheelMove) == 1);
  auto up = kd(0x56, 1200); up.kind = EvKind::KeyUp;
  auto released = d.onEvent(up);
  CHECK(countEmits(released, Emit::Kind::WheelRelease) == 1);
  CHECK(released.injects.empty());
  CHECK(d.state() == S::Idle);

  // A quick tap still types V once, without ever opening a wheel.
  d.onEvent(kd(0x56, 1300)); up.timeMs = 1350;
  released = d.onEvent(up);
  CHECK(released.injects.size() == 2);
  CHECK(countEmits(released, Emit::Kind::WheelRelease) == 0);
  CHECK(countEmits(d.tick(1500), Emit::Kind::WheelOpen) == 0);

  // Command+V is untouched.
  d.onEvent(kd(0x5B, 1600));
  CHECK(!d.onEvent(kd(0x56, 1610)).swallow);
  up.timeMs = 1620;
  CHECK(!d.onEvent(up).swallow);
  auto cmdUp = kd(0x5B, 1630); cmdUp.kind = EvKind::KeyUp; d.onEvent(cmdUp);

  // Esc cancels a held wheel and never leaks a V or leaves the state stuck.
  d.onEvent(kd(0x56, 1700)); d.tick(1850);
  CHECK(countEmits(d.onEvent(kd(0x1B, 1860)), Emit::Kind::Cancel) == 1);
  up.timeMs = 1900;
  CHECK(d.onEvent(up).injects.empty());
  CHECK(d.state() == S::Idle);
  d.onEvent(kd(0x56, 2000));
  CHECK(countEmits(d.tick(2150), Emit::Kind::WheelOpen) == 1);
  up.timeMs = 2200;
  CHECK(d.onEvent(up).injects.empty());
}

void testLauncherScrollStaysCapturedDuringThrottle() {
  Decision d = launcher();
  d.onEvent(md(Btn::X1, 400, 300, 1000));
  auto scroll = mv(400, 300, 1010); scroll.kind = EvKind::Scroll; scroll.delta = -1;
  const auto first = d.onEvent(scroll);
  CHECK(first.swallow);
  CHECK(countEmits(first, Emit::Kind::Scroll) == 1);
  scroll.timeMs = 1020;
  const auto next = d.onEvent(scroll);
  CHECK(next.swallow);
  CHECK(countEmits(next, Emit::Kind::Scroll) == 0);
}

}  // namespace

int main() {
  testOpenOnPress();
  testOpenThenImmediateRelease();
  testDragThresholdWithoutOpenOnPress();
  testCancelByRightClick();
  testWatchdog();
  testSetEnabledCancels();
  testKeyTriggerStillNeedsClick();
  testConfigCommand();
  testMacInputMapping();
  testHoldVLauncher();
  testLauncherScrollStaysCapturedDuringThrottle();
  std::printf("%d passed, %d failed\n", g_pass, g_fail);
  return g_fail == 0 ? 0 : 1;
}
