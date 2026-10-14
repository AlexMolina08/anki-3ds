// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

/* Native entropy source. psInit() must succeed before any TLS operation. */
#include <3ds.h>
#include <mbedtls/entropy.h>
#include <stddef.h>

int mbedtls_hardware_poll(void *data, unsigned char *output, size_t length,
                          size_t *written)
{
    (void)data;
    if (!written)
        return MBEDTLS_ERR_ENTROPY_SOURCE_FAILED;
    *written = 0;
    if (R_FAILED(PS_GenerateRandomBytes(output, length)))
        return MBEDTLS_ERR_ENTROPY_SOURCE_FAILED;
    *written = length;
    return 0;
}

/* Monotonic TLS timer; certificate validity still uses the console wall clock. */
#include <mbedtls/platform_time.h>
mbedtls_ms_time_t mbedtls_ms_time(void)
{
    return (mbedtls_ms_time_t)(svcGetSystemTick() / (SYSCLOCK_ARM11 / 1000));
}
