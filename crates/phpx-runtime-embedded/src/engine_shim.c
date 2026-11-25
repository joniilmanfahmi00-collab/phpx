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
 * Request isolation is a separate gate: sequential evals share one executor
 * until the reset proof lands, so callers must not treat evals as isolated
 * requests yet. See docs/operations/embedded.md.
 */
#include <sapi/embed/php_embed.h>
#include <Zend/zend_API.h>
#include <Zend/zend_execute.h>
#include <Zend/zend_exceptions.h>
#include <main/php_main.h>

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* Swap space for one in-flight eval (single-entry domain: at most one). */
static char *g_out = NULL;
static size_t g_out_len = 0;
static size_t g_out_cap = 0;

/* Hard cap: captured output never exceeds MAX_FRAME_BYTES (containment). */
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

/* 0 ok, 2 init failed. Idempotent: second call is a no-op success. */
int phpx_embed_startup(void) {
    static int started = 0;
    static char *fake_argv[] = {(char *)"phpx-embed", NULL};
    if (started) {
        return 0;
    }
    if (php_embed_init(1, fake_argv) != SUCCESS) {
        return 2;
    }
    php_embed_module.ub_write = phpx_ub_write;
    started = 1;
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
