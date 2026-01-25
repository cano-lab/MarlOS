/**
 * Semantic Hook Library
 * =====================
 *
 * LD_PRELOAD library that intercepts libc functions and reports
 * them to the Semantic OS translation layer via Unix socket.
 *
 * Compile:
 *   gcc -shared -fPIC -o libsemantic_hook.so semantic_hook.c -ldl
 *
 * Usage:
 *   LD_PRELOAD=./libsemantic_hook.so SEMANTIC_HOOK_SOCKET=/tmp/hook.sock ./app
 */

#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <errno.h>
#include <stdarg.h>

/* Configuration */
static char *hook_socket_path = NULL;
static int hook_enabled = 1;
static int hook_initialized = 0;

/* Original function pointers */
static int (*real_open)(const char *pathname, int flags, ...) = NULL;
static int (*real_close)(int fd) = NULL;
static ssize_t (*real_read)(int fd, void *buf, size_t count) = NULL;
static ssize_t (*real_write)(int fd, const void *buf, size_t count) = NULL;
static FILE* (*real_fopen)(const char *pathname, const char *mode) = NULL;
static int (*real_fclose)(FILE *stream) = NULL;
static int (*real_rename)(const char *oldpath, const char *newpath) = NULL;
static int (*real_unlink)(const char *pathname) = NULL;

/* File descriptor tracking (simple implementation) */
#define MAX_FDS 1024
static char *fd_paths[MAX_FDS] = {0};

/* Initialize the hook library */
static void hook_init(void) {
    if (hook_initialized) return;
    hook_initialized = 1;

    /* Get socket path from environment */
    hook_socket_path = getenv("SEMANTIC_HOOK_SOCKET");
    if (!hook_socket_path) {
        hook_enabled = 0;
        return;
    }

    /* Load real functions */
    real_open = dlsym(RTLD_NEXT, "open");
    real_close = dlsym(RTLD_NEXT, "close");
    real_read = dlsym(RTLD_NEXT, "read");
    real_write = dlsym(RTLD_NEXT, "write");
    real_fopen = dlsym(RTLD_NEXT, "fopen");
    real_fclose = dlsym(RTLD_NEXT, "fclose");
    real_rename = dlsym(RTLD_NEXT, "rename");
    real_unlink = dlsym(RTLD_NEXT, "unlink");

    if (!real_open || !real_close || !real_read || !real_write) {
        fprintf(stderr, "[semantic_hook] Failed to load real functions\n");
        hook_enabled = 0;
    }
}

/* Send message to Python layer */
static void send_hook_message(const char *json_msg) {
    if (!hook_enabled || !hook_socket_path) return;

    int sock = socket(AF_UNIX, SOCK_STREAM, 0);
    if (sock < 0) return;

    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, hook_socket_path, sizeof(addr.sun_path) - 1);

    if (connect(sock, (struct sockaddr*)&addr, sizeof(addr)) == 0) {
        /* Use real_write to avoid recursion */
        if (real_write) {
            real_write(sock, json_msg, strlen(json_msg));
        } else {
            write(sock, json_msg, strlen(json_msg));
        }
    }

    close(sock);
}

/* Report an intercepted call */
static void report_call(const char *api, const char *args_json) {
    char msg[4096];
    snprintf(msg, sizeof(msg),
        "{\"api\":\"%s\",\"args\":%s,\"pid\":%d}",
        api, args_json, getpid());
    send_hook_message(msg);
}

/* Determine flags string for reporting */
static const char* flags_to_string(int flags) {
    static char buf[256];
    buf[0] = '\0';

    if (flags & O_RDONLY) strcat(buf, "O_RDONLY");
    else if (flags & O_WRONLY) strcat(buf, "O_WRONLY");
    else if (flags & O_RDWR) strcat(buf, "O_RDWR");

    if (flags & O_CREAT) strcat(buf, "|O_CREAT");
    if (flags & O_TRUNC) strcat(buf, "|O_TRUNC");
    if (flags & O_APPEND) strcat(buf, "|O_APPEND");

    return buf;
}

/* ============================================================
 * Hooked Functions
 * ============================================================ */

int open(const char *pathname, int flags, ...) {
    hook_init();

    /* Handle optional mode argument */
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, mode_t);
        va_end(args);
    }

    /* Call real function first */
    int fd = real_open ? real_open(pathname, flags, mode) : -1;

    /* Report the call */
    if (hook_enabled && fd >= 0) {
        /* Track fd -> path mapping */
        if (fd < MAX_FDS) {
            if (fd_paths[fd]) free(fd_paths[fd]);
            fd_paths[fd] = strdup(pathname);
        }

        char args_json[1024];
        snprintf(args_json, sizeof(args_json),
            "{\"path\":\"%s\",\"flags\":%d,\"flags_str\":\"%s\",\"fd\":%d}",
            pathname, flags, flags_to_string(flags), fd);
        report_call("open", args_json);
    }

    return fd;
}

int close(int fd) {
    hook_init();

    /* Report before closing */
    if (hook_enabled && fd >= 0 && fd < MAX_FDS && fd_paths[fd]) {
        char args_json[512];
        snprintf(args_json, sizeof(args_json),
            "{\"fd\":%d,\"path\":\"%s\"}", fd, fd_paths[fd]);
        report_call("close", args_json);

        free(fd_paths[fd]);
        fd_paths[fd] = NULL;
    }

    return real_close ? real_close(fd) : -1;
}

ssize_t read(int fd, void *buf, size_t count) {
    hook_init();

    ssize_t result = real_read ? real_read(fd, buf, count) : -1;

    /* Only report significant reads (not stdin) */
    if (hook_enabled && result > 0 && fd > 2 && fd < MAX_FDS && fd_paths[fd]) {
        char args_json[512];
        snprintf(args_json, sizeof(args_json),
            "{\"fd\":%d,\"bytes\":%zd,\"path\":\"%s\"}",
            fd, result, fd_paths[fd]);
        report_call("read", args_json);
    }

    return result;
}

ssize_t write(int fd, const void *buf, size_t count) {
    hook_init();

    ssize_t result = real_write ? real_write(fd, buf, count) : -1;

    /* Only report significant writes (not stdout/stderr) */
    if (hook_enabled && result > 0 && fd > 2 && fd < MAX_FDS && fd_paths[fd]) {
        char args_json[512];
        snprintf(args_json, sizeof(args_json),
            "{\"fd\":%d,\"bytes\":%zd,\"path\":\"%s\"}",
            fd, result, fd_paths[fd]);
        report_call("write", args_json);
    }

    return result;
}

FILE* fopen(const char *pathname, const char *mode) {
    hook_init();

    FILE *fp = real_fopen ? real_fopen(pathname, mode) : NULL;

    if (hook_enabled && fp) {
        char args_json[1024];
        snprintf(args_json, sizeof(args_json),
            "{\"path\":\"%s\",\"mode\":\"%s\"}", pathname, mode);
        report_call("fopen", args_json);
    }

    return fp;
}

int fclose(FILE *stream) {
    hook_init();

    if (hook_enabled && stream) {
        char args_json[128];
        snprintf(args_json, sizeof(args_json),
            "{\"stream\":\"%p\"}", (void*)stream);
        report_call("fclose", args_json);
    }

    return real_fclose ? real_fclose(stream) : -1;
}

int rename(const char *oldpath, const char *newpath) {
    hook_init();

    int result = real_rename ? real_rename(oldpath, newpath) : -1;

    if (hook_enabled) {
        char args_json[2048];
        snprintf(args_json, sizeof(args_json),
            "{\"oldpath\":\"%s\",\"newpath\":\"%s\",\"success\":%s}",
            oldpath, newpath, result == 0 ? "true" : "false");
        report_call("rename", args_json);
    }

    return result;
}

int unlink(const char *pathname) {
    hook_init();

    int result = real_unlink ? real_unlink(pathname) : -1;

    if (hook_enabled) {
        char args_json[1024];
        snprintf(args_json, sizeof(args_json),
            "{\"path\":\"%s\",\"success\":%s}",
            pathname, result == 0 ? "true" : "false");
        report_call("unlink", args_json);
    }

    return result;
}

/* Constructor - runs when library is loaded */
__attribute__((constructor))
static void on_load(void) {
    hook_init();
    if (hook_enabled) {
        char args_json[256];
        snprintf(args_json, sizeof(args_json),
            "{\"event\":\"library_loaded\",\"pid\":%d}", getpid());
        report_call("__init__", args_json);
    }
}

/* Destructor - runs when library is unloaded */
__attribute__((destructor))
static void on_unload(void) {
    if (hook_enabled) {
        char args_json[256];
        snprintf(args_json, sizeof(args_json),
            "{\"event\":\"library_unloaded\",\"pid\":%d}", getpid());
        report_call("__exit__", args_json);
    }

    /* Cleanup fd tracking */
    for (int i = 0; i < MAX_FDS; i++) {
        if (fd_paths[i]) {
            free(fd_paths[i]);
            fd_paths[i] = NULL;
        }
    }
}
