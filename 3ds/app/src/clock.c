// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#include <time.h>
#include <stdint.h>

/* Horizon's RTC is local civil time and carries no time zone. */
static int64_t utc_offset_seconds;
void anki_set_clock_correction(int64_t seconds) { utc_offset_seconds = seconds; }
int __real_clock_gettime(clockid_t id, struct timespec *value);
int __wrap_clock_gettime(clockid_t id, struct timespec *value) {
    int result = __real_clock_gettime(id, value);
    if (!result && id == CLOCK_REALTIME) value->tv_sec -= utc_offset_seconds;
    return result;
}
time_t __real_time(time_t *value);
int64_t anki_local_time(void) { return (int64_t)__real_time(NULL); }
time_t __wrap_time(time_t *value) {
    time_t result = __real_time(NULL);
    if (result != (time_t)-1) result -= utc_offset_seconds;
    if (value) *value = result;
    return result;
}
