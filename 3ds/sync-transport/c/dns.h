// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#ifndef ANKI_3DS_PROBE_DNS_H
#define ANKI_3DS_PROBE_DNS_H
#include <stdbool.h>
/* Requires initialized SOC and PS. Caller measures elapsed time with osGetTime().
 * Uses configured console DNS. Optional explicit emulator configuration:
 * sdmc:/3ds/anki-https-probe/dns-server.txt containing one IPv4 line.
 * Never include that override in the physical hardware package.
 */
int probe_dns_with_context(const char *host, char ip[16], char error[160],
                           bool (*cancelled)(void *), void *context, bool allow_override);
int probe_dns(const char *host, char ip[16], char error[160], bool (*cancelled)(void));
#endif
