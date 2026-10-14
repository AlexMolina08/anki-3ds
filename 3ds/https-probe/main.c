// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#include <3ds.h>
#include <curl/curl.h>
#include <mbedtls/version.h>
#include <malloc.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <time.h>
#include <sys/stat.h>
#include "dns.h"
#include "meta_body.h"

#ifndef GIT_HASH
#define GIT_HASH "unknown"
#endif

#define REPORT "sdmc:/3ds/anki-https-probe/report.txt"
#define CA_FILE "sdmc:/3ds/anki-https-probe/cacert.pem"
#define BODY_LIMIT (256 * 1024)
static LightLock state_lock;
static bool cancel_requested, finished;
static char status[96] = "Starting services";
static FILE *report;
static size_t sampled_heap_peak;
static unsigned tests_passed, tests_failed;

static bool cancelled(void) {
    LightLock_Lock(&state_lock);
    bool value = cancel_requested;
    LightLock_Unlock(&state_lock);
    return value;
}
static void set_status(const char *text) {
    LightLock_Lock(&state_lock);
    snprintf(status, sizeof(status), "%s", text);
    LightLock_Unlock(&state_lock);
}
static void sample_heap(void) {
    size_t used = mallinfo().uordblks;
    if (used > sampled_heap_peak) sampled_heap_peak = used;
}
struct Transfer {
    size_t bytes;
    unsigned long hash;
    char tls[48];
    unsigned debug_lines;
    char echo[4097];
    size_t echo_size;
    bool body_limit;
    u64 deadline;
    u64 start_time;
    u64 tcp_time;
    u64 tls_time;
    u64 upload_time;
    u64 first_byte_time;
};
static size_t receive_body(char *data, size_t size, size_t count, void *context) {
    struct Transfer *t = context;
    if (size && count > SIZE_MAX / size) return 0;
    size_t n = size * count;
    if (n > 0 && t->first_byte_time == 0) {
        t->first_byte_time = osGetTime();
    }
    if (n > BODY_LIMIT - t->bytes) { t->body_limit = true; return 0; }
    for (size_t i = 0; i < n; i++) t->hash = (t->hash ^ (unsigned char)data[i]) * 16777619UL;
    t->bytes += n;
    size_t copy = n < sizeof(t->echo)-1-t->echo_size ? n : sizeof(t->echo)-1-t->echo_size;
    memcpy(t->echo+t->echo_size, data, copy);
    t->echo_size += copy;
    t->echo[t->echo_size] = 0;
    sample_heap();
    return cancelled() ? 0 : n;
}
static int progress(void *context, curl_off_t dltotal, curl_off_t dlnow, curl_off_t ultotal, curl_off_t ulnow) {
    (void)dltotal; (void)dlnow;
    struct Transfer *t = context;
    if (ultotal > 0 && ulnow >= ultotal && t->upload_time == 0) {
        t->upload_time = osGetTime();
    }
    sample_heap();
    return cancelled() || osGetTime() >= t->deadline;
}
static int diagnostic(CURL *handle, curl_infotype kind, char *data, size_t size, void *context) {
    (void)handle;
    struct Transfer *t = context;
    if (kind == CURLINFO_TEXT && size < 256) {
        char line[256];
        memcpy(line, data, size); line[size] = 0;
        if (t->debug_lines < 12 &&
            (strstr(line,"connect") || strstr(line,"socket") || strstr(line,"Trying") || strstr(line,"failed"))) {
            fprintf(report,"transport=%.*s",(int)size,line);
            t->debug_lines++;
        }
        if (strstr(line, "Connected to") || strstr(line, "Established connection")) {
            if (t->tcp_time == 0) t->tcp_time = osGetTime();
        }
        const char *p = strstr(line, "mbedTLS: TLSv");
        if (p && strstr(p, "Handshake complete")) {
            if (t->tls_time == 0) t->tls_time = osGetTime();
            p += strlen("mbedTLS: ");
            size_t len = strcspn(p, " \r\n");
            if (len < sizeof(t->tls)) {
                memcpy(t->tls, p, len);
                t->tls[len] = 0;
            }
        }
    }
    return 0;
}
static void test_request(const char *label, const char *host, const char *path,
                         const unsigned char *body, size_t body_size, int expected, long timeout_ms) {
    if (cancelled()) return;
    set_status(label);
    char ip[16], dns_error[160] = "", url[256], mapping[160], error[CURL_ERROR_SIZE] = "";
    u64 start = osGetTime();
    int dns = probe_dns(host, ip, dns_error, cancelled);
    u64 dns_end = osGetTime();
    fprintf(report, "\n[%s]\ndns=%s dns_ms=%llu\n", label, dns ? dns_error : ip,
            (unsigned long long)(dns_end - start));
    if (dns) { tests_failed++; fflush(report); return; }
    CURL *easy = curl_easy_init();
    struct curl_slist *resolve = NULL, *headers = NULL;
    struct Transfer transfer = {
        .hash=2166136261UL,
        .deadline=osGetTime()+(u64)timeout_ms,
        .start_time=start
    };
    if (!easy) { fprintf(report,"error=curl_easy_init\n"); tests_failed++; return; }
    snprintf(url,sizeof(url),"https://%s%s",host,path);
    snprintf(mapping,sizeof(mapping),"%s:443:%s",host,ip);
    resolve = curl_slist_append(NULL,mapping);
    headers = curl_slist_append(NULL,"Content-Type: application/octet-stream");
    if (!strcmp(host,"sync.ankiweb.net"))
        headers = curl_slist_append(headers,"anki-sync: {\"v\":11,\"k\":\"\",\"c\":\"anki-3ds-https-probe\",\"s\":\"httpsprobe\"}");
    CURLcode code = CURLE_OK;
#define SET(option,value) do { CURLcode rc=curl_easy_setopt(easy,option,value); if(rc != CURLE_OK) { fprintf(report,"option_failed=%s\n",#option); code=rc; goto cleanup; } } while(0)
    if (!resolve || !headers) { code=CURLE_OUT_OF_MEMORY; goto cleanup; }
    SET(CURLOPT_URL,url);
    SET(CURLOPT_RESOLVE,resolve);
    SET(CURLOPT_PROTOCOLS_STR,"https");
    SET(CURLOPT_REDIR_PROTOCOLS_STR,"https");
    SET(CURLOPT_FOLLOWLOCATION,0L);
    SET(CURLOPT_HTTP_VERSION,(long)CURL_HTTP_VERSION_1_1);
    SET(CURLOPT_SSLVERSION,(long)CURL_SSLVERSION_TLSv1_2);
    SET(CURLOPT_SSL_VERIFYPEER,1L);
    SET(CURLOPT_SSL_VERIFYHOST,2L);
    SET(CURLOPT_CAINFO,CA_FILE);
    SET(CURLOPT_CONNECTTIMEOUT_MS,15000L);
    SET(CURLOPT_TIMEOUT_MS,timeout_ms);
    SET(CURLOPT_NOSIGNAL,1L);
    SET(CURLOPT_BUFFERSIZE,8192L);
    SET(CURLOPT_WRITEFUNCTION,receive_body);
    SET(CURLOPT_WRITEDATA,&transfer);
    SET(CURLOPT_NOPROGRESS,0L);
    SET(CURLOPT_XFERINFOFUNCTION,progress);
    SET(CURLOPT_XFERINFODATA,&transfer);
    SET(CURLOPT_ERRORBUFFER,error);
    SET(CURLOPT_VERBOSE,1L);
    SET(CURLOPT_DEBUGFUNCTION,diagnostic);
    SET(CURLOPT_DEBUGDATA,&transfer);
    SET(CURLOPT_HTTPHEADER,headers);
    SET(CURLOPT_USERAGENT,"Anki3DS-HTTPS-Probe/1");
    if (body) {
        SET(CURLOPT_POST,1L);
        SET(CURLOPT_POSTFIELDS,(const char *)body);
        SET(CURLOPT_POSTFIELDSIZE_LARGE,(curl_off_t)body_size);
    }
    sample_heap();
    code=curl_easy_perform(easy);
    sample_heap();
cleanup: ;
    long http=0, verification=-1;
    double duration=0, handshake=0, connect_time=0;
    curl_off_t upload=0;
    curl_easy_getinfo(easy,CURLINFO_RESPONSE_CODE,&http);
    curl_easy_getinfo(easy,CURLINFO_SSL_VERIFYRESULT,&verification);
    curl_easy_getinfo(easy,CURLINFO_TOTAL_TIME,&duration);
    curl_easy_getinfo(easy,CURLINFO_CONNECT_TIME,&connect_time);
    curl_easy_getinfo(easy,CURLINFO_APPCONNECT_TIME,&handshake);
    curl_easy_getinfo(easy,CURLINFO_SIZE_UPLOAD_T,&upload);

    u64 tcp_ms = transfer.tcp_time ? (transfer.tcp_time - start) : (u64)(connect_time * 1000);
    u64 tls_ms = transfer.tls_time ? (transfer.tls_time - start) : (u64)(handshake * 1000);
    u64 upload_ms = transfer.upload_time ? (transfer.upload_time - start) : 0;
    u64 first_byte_ms = transfer.first_byte_time ? (transfer.first_byte_time - start) : 0;

    fprintf(report, "stages: dns_done=%llu tcp_conn=%llu tls_done=%llu upload_done=%llu first_rx=%llu total=%llu\n",
            (unsigned long long)(dns_end - start),
            (unsigned long long)tcp_ms,
            (unsigned long long)tls_ms,
            (unsigned long long)upload_ms,
            (unsigned long long)first_byte_ms,
            (unsigned long long)(osGetTime() - start));

    bool tls_ok=transfer.tls[0] && verification==0;
    const char *cert_status=code==CURLE_PEER_FAILED_VERIFICATION?"rejected":tls_ok?"verified":"not-completed";
    bool pass=code==CURLE_OK && tls_ok && http>0;
    if (expected==1) pass=code==CURLE_PEER_FAILED_VERIFICATION && http==0;
    if (expected==2) pass=pass && transfer.bytes==32;
    if (expected==3) pass=pass && strstr(transfer.echo,"AAH/f0EK")!=NULL;
    if (expected==4) pass=code==CURLE_OPERATION_TIMEDOUT || code==CURLE_ABORTED_BY_CALLBACK;
    fprintf(report,"tls=%s handshake_ms=%.0f certificate=%s cert_result=%ld http=%ld\n"
        "curl_code=%d error=%s\nrequest_ms=%.0f elapsed_ms=%llu upload_bytes=%lld response_bytes=%u\n"
        "response_fnv1a=%08lx body_limit=%d heap_sample_peak=%u result=%s\n",
        transfer.tls[0]?transfer.tls:"not-established",handshake*1000,cert_status,verification,http,
        code,error[0]?error:curl_easy_strerror(code),duration*1000,
        (unsigned long long)(osGetTime()-start),(long long)upload,(unsigned)transfer.bytes,
        transfer.hash,transfer.body_limit,(unsigned)sampled_heap_peak,pass?"PASS":"FAIL");
    fflush(report);
    if(pass) tests_passed++; else tests_failed++;
    curl_slist_free_all(headers); curl_slist_free_all(resolve); curl_easy_cleanup(easy);
#undef SET
}
static void worker(void *unused) {
    (void)unused;
    CURLcode rc=curl_global_init(CURL_GLOBAL_DEFAULT);
    if (rc!=CURLE_OK) fprintf(report,"curl_global_init=%d\n",rc);
    else {
        fprintf(report,"curl=%s\n",curl_version());
        test_request("Public HTTPS","example.com","/",NULL,0,0,25000);
        static const unsigned char binary[]={0,1,255,127,'A','\n'};
        test_request("Binary POST echo","httpbin.org","/post",binary,sizeof(binary),3,25000);
        test_request("Binary response","httpbin.org","/bytes/32",NULL,0,2,25000);
        test_request("AnkiWeb unauthenticated meta","sync.ankiweb.net","/sync/meta",meta_body,sizeof(meta_body),0,60000);
        test_request("Reject expired certificate","expired.badssl.com","/",NULL,0,1,25000);
        test_request("Reject wrong hostname","wrong.host.badssl.com","/",NULL,0,1,25000);
        test_request("Transfer deadline","example.com","/",NULL,0,4,1);
        curl_global_cleanup();
    }
    fprintf(report,"\ncomplete=1 cancelled=%d pass=%u fail=%u sampled_heap_peak=%u\n"
            "Memory is sampled allocated heap, not total RAM. Physical hardware remains unverified.\n",
            cancelled(),tests_passed,tests_failed,(unsigned)sampled_heap_peak);
    bool write_failed=ferror(report)!=0;
    if(fclose(report)!=0 || write_failed) set_status("SD report write failed");
    else {
        char summary[96];
        snprintf(summary,sizeof(summary),"%s: %u pass, %u fail; SD saved",cancelled()?"Cancelled":"Done",tests_passed,tests_failed);
        set_status(summary);
    }
    LightLock_Lock(&state_lock); finished=true; LightLock_Unlock(&state_lock);
}
int main(void) {
    gfxInitDefault(); consoleInit(GFX_TOP,NULL); LightLock_Init(&state_lock);
    mkdir("sdmc:/3ds",0777); mkdir("sdmc:/3ds/anki-https-probe",0777);
    report=fopen(REPORT,"w");
    void *soc_buffer=memalign(4096,1024*1024);
    bool soc_ready=false, ps_ready=false;
    Thread thread=NULL;
    Result soc_result=soc_buffer?socInit(soc_buffer,1024*1024):-1;
    soc_ready=R_SUCCEEDED(soc_result);
    Result ps_result=psInit(); ps_ready=R_SUCCEEDED(ps_result);
    if(report) fprintf(report,"Anki 3DS HTTPS probe v3 (build %s)\nunix_time=%lld soc=0x%08lx ps=0x%08lx\n"
            "soc_buffer_bytes=1048576 worker_stack_bytes=262144\n"
            "peer_validation=required hostname_validation=required TLS_min=1.2 redirects=disabled\n",
            GIT_HASH,(long long)time(NULL),(unsigned long)soc_result,(unsigned long)ps_result);
    if(report) {
        FILE *override=fopen("sdmc:/3ds/anki-https-probe/dns-server.txt","r");
        fprintf(report,"dns_source=%s\n",override?"explicit SD override":"console Wi-Fi configuration");
        if(override) fclose(override);
    }
    if(report && soc_ready && ps_ready) {
        thread=threadCreate(worker,NULL,256*1024,0x30,-2,false);
        if(!thread) { fprintf(report,"error=threadCreate\n"); fclose(report); finished=true; set_status("Worker creation failed"); }
    } else {
        if(report) { fprintf(report,"error=service initialization\ncomplete=0\n"); fclose(report); }
        finished=true; set_status(report?"Network/RNG service failed":"Cannot write SD report");
    }
    bool exit_requested=false;
    char previous[96]="";
    while(aptMainLoop()) {
        hidScanInput(); u32 keys=hidKeysDown();
        LightLock_Lock(&state_lock);
        if(keys & KEY_B) cancel_requested=true;
        if(keys & KEY_START) { cancel_requested=true; exit_requested=true; }
        bool done=finished; char display[96]; snprintf(display,sizeof(display),"%s",status);
        LightLock_Unlock(&state_lock);
        if(strcmp(previous,display)) {
        snprintf(previous,sizeof(previous),"%s",display);
        consoleClear();
        printf("Anki 3DS secure HTTPS test v3\n\n%s\n\n",display);
        printf("Own Wi-Fi; no account or collection.\n\nB: cancel tests\nSTART: cancel, then exit safely\n\n");
        printf("Report:\n/3ds/anki-https-probe/report.txt\n\nKeep console awake during testing.\n");
        }
        if(exit_requested && done) break;
        gfxFlushBuffers(); gfxSwapBuffers(); gspWaitForVBlank();
    }
    LightLock_Lock(&state_lock); cancel_requested=true; LightLock_Unlock(&state_lock);
    if(thread) { threadJoin(thread,U64_MAX); threadFree(thread); }
    if(ps_ready) psExit();
    if(soc_ready) socExit();
    free(soc_buffer);
    FILE *cleanup_report=fopen(REPORT,"a");
    if(cleanup_report) {
        fprintf(cleanup_report,"cleanup=1 worker_joined=%d ps_closed=%d soc_closed=%d\n",thread!=NULL,ps_ready,soc_ready);
        fclose(cleanup_report);
    }
    gfxExit(); return 0;
}
