/* PHPX embedded engine shim (P2 promotion: real libphp execution path).
 *
 * Compiled ONLY for linked Linux NTS builds (PHPX_EMBED_LINK=1 + libphp
 * found); stub builds never compile this file. Keeps every ABI-dangerous
 * operation in C, where setjmp/longjmp bailouts are natural:
 *  - engine init/shutdown around php-embed,
 *  - ub_write capture into a bounded buffer,
 *  - eval wrapped in zend_first_try/zend_catch (a fatal inside eval
 *    longjmps out instead of aborting the host process),
 *  - per-call output handoff (caller frees with phpx_embed_free).
 *
 * Request isolation: sequential evals share one executor until the caller
 * cycles the request (phpx_embed_next_request); callers must not treat
 * bare evals as isolated requests. See docs/operations/embedded.md.
 */
#include <sapi/embed/php_embed.h>
#include <Zend/zend_API.h>
#include <Zend/zend_execute.h>
#include <Zend/zend_exceptions.h>
#include <main/php_main.h>
#include <main/php_variables.h>
#include <main/php_globals.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Forward declarations (SAPI plumbing lives below the eval core). */
static void phpx_clear_request_state(void);
static void phpx_register_server_variables(zval *track_vars_array);

/* Swap space for one in-flight eval (single-entry domain: at most one). */
static char *g_out = NULL;
static size_t g_out_len = 0;
static size_t g_out_cap = 0;

/* Captured output never exceeds MAX_FRAME_BYTES (containment). */
#define PHPX_OUT_CAP ((size_t)16 * 1024 * 1024)

static size_t phpx_ub_write(const char *str, size_t len) {
    if (len == 0) {
        return 0;
    }
    if (g_out_len + len + 1 > g_out_cap) {
        size_t ncap = g_out_cap ? g_out_cap * 2 : 4096;
        while (ncap < g_out_len + len + 1) {
            ncap *= 2;
        }
        if (ncap > PHPX_OUT_CAP + 1024) {
            return 0;
        }
        char *nbuf = (char *)realloc(g_out, ncap);
        if (!nbuf) {
            return 0;
        }
        g_out = nbuf;
        g_out_cap = ncap;
    }
    memcpy(g_out + g_out_len, str, len);
    g_out_len += len;
    g_out[g_out_len] = '\0';
    return len;
}

/* Engine liveness (file scope so request cycling can check it). */
static int g_started = 0;

/* NOTE: sapi_startup() COPIES php_embed_module into the global `sapi_module`;
 * ub_write calls go through the global copy, so the override must target
 * `sapi_module.ub_write` (overriding php_embed_module after startup is a
 * silent no-op — output would leak to real stdout, as the first live run
 * proved). Declared in main/SAPI.h as extern SAPI_API.
 */
extern sapi_module_struct sapi_module;

int phpx_embed_startup(void) {
    static char *fake_argv[] = {(char *)"phpx-embed", NULL};
    if (g_started) {
        return 0;
    }
    if (php_embed_init(1, fake_argv) != SUCCESS) {
        return 2;
    }
    sapi_module.ub_write = phpx_ub_write;
    sapi_module.register_server_variables = phpx_register_server_variables;
    sapi_module.treat_data = php_default_treat_data;
    sapi_module.input_filter = php_default_input_filter;
    g_started = 1;
    return 0;
}

/* Codes: 0 ok, 1 bailout/eval-failed (host survives), 2 not started.
 *
 * NOTE (from zend_execute_API.c): when retval is requested the engine wraps
 * the snippet as `return <code>;` itself, so callers MUST pass a bare
 * expression (e.g. `6*7`, not `return 6*7;`). Statements (`echo ...;`) go
 * through with want_retval=0 (retval NULL).
 */
int phpx_embed_eval(const char *code, size_t code_len, int want_retval, char **outp, size_t *outlenp,
                    long *retlongp, int *retislongp) {
    if (!code || code_len == 0 || code_len > (size_t)1024 * 1024) {
        return 1;
    }
    g_out_len = 0;
    if (g_out) {
        g_out[0] = '\0';
    }
    if (outp) {
        *outp = NULL;
    }
    if (outlenp) {
        *outlenp = 0;
    }
    if (retlongp) {
        *retlongp = 0;
    }
    if (retislongp) {
        *retislongp = 0;
    }

    int status = 1;
    zval retval;
    ZVAL_UNDEF(&retval);
    zval *retp = want_retval ? &retval : NULL;
    zend_first_try {
        zend_result zr = zend_eval_stringl_ex(code, code_len, retp, "phpx_embed_eval", 1);
        if (zr == SUCCESS) {
            status = 0;
            if (want_retval && Z_TYPE(retval) == IS_LONG) {
                if (retlongp) {
                    *retlongp = Z_LVAL(retval);
                }
                if (retislongp) {
                    *retislongp = 1;
                }
            }
        } else {
            /* Parse/eval failure: drop any half-built value, clear state. */
            if (Z_TYPE(retval) != IS_UNDEF) {
                zval_ptr_dtor(&retval);
                ZVAL_UNDEF(&retval);
            }
            zend_clear_exception();
            status = 1;
        }
    } zend_catch {
        /* Fatal inside eval (longjmp out of the executor). Host survives;
         * the value on the stack is indeterminate, so reset it. */
        ZVAL_UNDEF(&retval);
        zend_clear_exception();
        status = 1;
    } zend_end_try();

    if (Z_TYPE(retval) != IS_UNDEF) {
        zval_ptr_dtor(&retval);
    }
    if (g_out_len > 0 && outp && outlenp) {
        char *copy = (char *)malloc(g_out_len + 1);
        if (!copy) {
            return 1;
        }
        memcpy(copy, g_out, g_out_len + 1);
        *outp = copy;
        *outlenp = g_out_len;
    }
    return status;
}

void phpx_embed_free(void *p) {
    free(p);
}

/* ------------------------------------------------------------------ */
/* SAPI request plumbing: per-request CGI-like state for compat tests. */
/*                                                                     */
/* $_GET/$_COOKIE come from php_default_treat_data (the CGI mechanism) */
/* reading SG fields set per request. $_SERVER comes from the module's  */
/* register_server_variables hook, which runs on first compiled        */
/* reference and reads the per-request statics below. The auto-global   */
/* JIT wipes and re-derives state per request, so every request must   */
/* set SG + statics BEFORE the first app eval; next_request() clears   */
/* both (plus any estrdup'd SG strings) to avoid cross-request leaks.  */
/*                                                                     */
/* Known limits (documented, tested): no $_POST parsing (no sim reads  */
/* it), no multipart (bytes stay raw via $GLOBALS['PHPX_BODY']), no    */
/* PHP_AUTH_* (no sim needs it). $_FILES is never populated.           */
/* ------------------------------------------------------------------ */

#define PHPX_MAX_HDR 24
#define PHPX_MAX_HNAME 64
#define PHPX_MAX_HVAL 512

static char g_method[16];
static char g_uri[2048];
static char g_query[2048];
static char g_ctype[128];
static char g_clen[32];
static char g_cookie[2048];
static char g_hnames[PHPX_MAX_HDR][PHPX_MAX_HNAME];
static char g_hvals[PHPX_MAX_HDR][PHPX_MAX_HVAL];
static int g_nhdrs = 0;

static void phpx_clear_request_state(void) {
    g_method[0] = g_uri[0] = g_query[0] = g_ctype[0] = g_clen[0] = g_cookie[0] = '\0';
    g_nhdrs = 0;
    memset(g_hnames, 0, sizeof(g_hnames));
    memset(g_hvals, 0, sizeof(g_hvals));
    if (SG(request_info).request_method) {
        efree((void *)SG(request_info).request_method);
        SG(request_info).request_method = NULL;
    }
    if (SG(request_info).request_uri) {
        efree((void *)SG(request_info).request_uri);
        SG(request_info).request_uri = NULL;
    }
    if (SG(request_info).query_string) {
        efree((void *)SG(request_info).query_string);
        SG(request_info).query_string = NULL;
    }
    if (SG(request_info).cookie_data) {
        efree((void *)SG(request_info).cookie_data);
        SG(request_info).cookie_data = NULL;
    }
    SG(request_info).content_type = NULL;
    SG(request_info).content_length = 0;
}

static void phpx_register_server_variables(zval *track_vars_array) {
    php_import_environment_variables(track_vars_array);
    if (g_method[0]) {
        php_register_variable("REQUEST_METHOD", g_method, track_vars_array);
    }
    if (g_uri[0]) {
        php_register_variable("REQUEST_URI", g_uri, track_vars_array);
    }
    if (g_query[0]) {
        php_register_variable("QUERY_STRING", g_query, track_vars_array);
    }
    if (g_ctype[0]) {
        php_register_variable("CONTENT_TYPE", g_ctype, track_vars_array);
    }
    if (g_clen[0]) {
        php_register_variable("CONTENT_LENGTH", g_clen, track_vars_array);
    }
    if (g_cookie[0]) {
        php_register_variable("HTTP_COOKIE", g_cookie, track_vars_array);
    }
    php_register_variable("REMOTE_ADDR", "127.0.0.1", track_vars_array);
    php_register_variable("SERVER_NAME", "localhost", track_vars_array);
    php_register_variable("SERVER_PORT", "80", track_vars_array);
    for (int i = 0; i < g_nhdrs; i++) {
        char key[96];
        snprintf(key, sizeof(key), "HTTP_%s", g_hnames[i]);
        for (char *p = key; *p; p++) {
            if (*p >= 'a' && *p <= 'z') {
                *p -= 32;
            } else if (*p == '-') {
                *p = '_';
            }
        }
        php_register_variable(key, g_hvals[i], track_vars_array);
    }
}

/* Bind PG's array into the symbol table so app code sees THIS request's
 * values even if eager RINIT binding pointed at a stale array. Shared
 * (Z_TRY_ADDREF), never orphaned; the request cycle re-binds fresh. */
static void phpx_bind_pg(const char *name, int track_idx) {
    zval *pg = &PG(http_globals)[track_idx];
    if (Z_TYPE_P(pg) != IS_ARRAY) {
        return;
    }
    Z_TRY_ADDREF_P(pg);
    zend_hash_str_update(&EG(symbol_table), name, strlen(name), pg);
}

/* Start a fresh request: clear per-request state, cycle RINIT. The C
 * runtime keeps exactly one request active; callers must set request
 * state (below) before the first app eval, and must never add an extra
 * request_shutdown: embed_shutdown() performs the final RSHUTDOWN, and
 * an unbalanced extra one corrupts the heap at teardown (observed).
 * Returns 0 ok, 1 startup failed, 2 engine not started. */
int phpx_embed_next_request(void) {
    if (!g_started) {
        return 2;
    }
    phpx_clear_request_state();
    php_request_shutdown(NULL);
    if (php_request_startup() != SUCCESS) {
        return 1;
    }
    /* Match php_embed_init's posture: this SAPI never sends headers
     * (output goes through ub_write), so keep header() calls no-ops after
     * every cycle, not just after init. */
    SG(headers_sent) = 1;
    SG(request_info).no_headers = 1;
    return 0;
}

/* Set request line + query. Fills $_GET via the CGI mechanism and binds it
 * for app visibility. Pass ""/NULL for no query string. */
int phpx_embed_set_request(const char *method, const char *uri, const char *query) {
    if (!g_started) {
        return 2;
    }
    if (method && *method) {
        snprintf(g_method, sizeof(g_method), "%s", method);
        SG(request_info).request_method = estrdup(method);
    }
    if (uri && *uri) {
        snprintf(g_uri, sizeof(g_uri), "%s", uri);
        SG(request_info).request_uri = estrdup(uri);
    }
    if (query && *query) {
        snprintf(g_query, sizeof(g_query), "%s", query);
        SG(request_info).query_string = estrdup(query);
    }
    php_default_treat_data(PARSE_GET, NULL, NULL);
    phpx_bind_pg("_GET", TRACK_VARS_GET);
    return 0;
}

/* Set one request header. "Cookie" feeds $_COOKIE (split on ';', values
 * urldecoded, like CGI); "Content-Type" is remembered for the body;
 * everything else becomes HTTP_* (uppercased, '-' to '_'). Unknown count
 * is capped; extras beyond the cap are dropped (bounded, never grows). */
int phpx_embed_set_header(const char *name, const char *value) {
    char lname[64];
    size_t i;
    if (!g_started || !name || !value) {
        return g_started ? 0 : 2;
    }
    for (i = 0; i < sizeof(lname) - 1 && name[i]; i++) {
        char c = name[i];
        lname[i] = (c >= 'A' && c <= 'Z') ? (char)(c + 32) : c;
    }
    lname[i] = '\0';
    if (!strcmp(lname, "cookie")) {
        snprintf(g_cookie, sizeof(g_cookie), "%s", value);
        SG(request_info).cookie_data = estrdup(value);
        php_default_treat_data(PARSE_COOKIE, NULL, NULL);
        phpx_bind_pg("_COOKIE", TRACK_VARS_COOKIE);
        return 0;
    }
    if (!strcmp(lname, "content-type")) {
        snprintf(g_ctype, sizeof(g_ctype), "%s", value);
        SG(request_info).content_type = estrdup(value);
        return 0;
    }
    if (g_nhdrs >= PHPX_MAX_HDR) {
        return 0;
    }
    snprintf(g_hnames[g_nhdrs], sizeof(g_hnames[0]), "%s", name);
    snprintf(g_hvals[g_nhdrs], sizeof(g_hvals[0]), "%s", value);
    g_nhdrs++;
    return 0;
}

/* Stash the raw request body where app code expects it
 * ($GLOBALS['PHPX_BODY'], binary-safe) and record its length.
 * No $_POST parsing by design (see header comment). */
int phpx_embed_set_body(const char *bytes, size_t len) {
    zval v;
    if (!g_started) {
        return 2;
    }
    snprintf(g_clen, sizeof(g_clen), "%zu", len);
    SG(request_info).content_length = (zend_long)len;
    if (bytes && len > 0) {
        ZVAL_STRINGL(&v, bytes, len);
    } else {
        ZVAL_EMPTY_STRING(&v);
    }
    zend_hash_str_update(&EG(symbol_table), "PHPX_BODY", sizeof("PHPX_BODY") - 1, &v);
    return 0;
}

/* Define the phpx_input() polyfill for the current request (user functions
 * die with every request cycle, so this is re-defined per request). */
int phpx_embed_define_helpers(void) {
    static const char code[] =
        "if(!function_exists('phpx_input')){function phpx_input(){"
        "return isset($GLOBALS['PHPX_BODY'])?(string)$GLOBALS['PHPX_BODY']:'';}}";
    char *out = NULL;
    size_t outlen = 0;
    int rc = phpx_embed_eval(code, sizeof(code) - 1, 0, &out, &outlen, NULL, NULL);
    if (out) {
        free(out);
    }
    return rc == 0 ? 0 : 1;
}

/* Evaluate `include '<abs path>';` with output capture. The path is
 * single-quote escaped (only backslash and quote are special there). */
int phpx_embed_run_include(const char *abs_path, char **outp, size_t *outlenp) {
    char code[4352];
    size_t n = 0;
    const char *head = "include '";
    const char *tail = "';";
    size_t hn = strlen(head), tn = strlen(tail);
    if (!abs_path || !outp || !outlenp) {
        return 1;
    }
    if (strlen(abs_path) > 4096) {
        return 1;
    }
    memcpy(code + n, head, hn);
    n += hn;
    for (const char *p = abs_path; *p && n + 2 < sizeof(code) - tn - 1; p++) {
        if (*p == '\\' || *p == '\'') {
            code[n++] = '\\';
        }
        code[n++] = *p;
    }
    memcpy(code + n, tail, tn);
    n += tn;
    code[n] = '\0';
    return phpx_embed_eval(code, n, 0, outp, outlenp, NULL, NULL);
}
