import sys
import os

def log(msg):
    print(msg, file=sys.stderr, flush=True)

log(f"captcha_webview.py starting, python={sys.executable}")
log(f"argv={sys.argv}")
log(f"DISPLAY={os.environ.get('DISPLAY','<unset>')} WAYLAND_DISPLAY={os.environ.get('WAYLAND_DISPLAY','<unset>')}")

try:
    import json
    import traceback
    import gi
    log("gi imported ok")
    gi.require_version('WebKit2', '4.1')
    log("WebKit2 version set")
    from gi.repository import WebKit2, Gtk, GLib
    log("WebKit2, Gtk, GLib imported ok")
except Exception as e:
    import traceback as tb
    log(f"IMPORT FAILED: {e}")
    log(tb.format_exc())
    sys.exit(1)

HV_TOKEN = sys.argv[2] if len(sys.argv) > 2 else ""
log(f"HV_TOKEN present: {bool(HV_TOKEN)}")


def find_final_token(data):
    """Extract the combined HV token from captcha completion messages.

    Accepted formats:
      pm_captcha:                 {type: 'pm_captcha', token: '...'}
      HUMAN_VERIFICATION_SUCCESS: {type: 'HUMAN_VERIFICATION_SUCCESS', payload: {token: '...'}}
      proton_captcha (combined):  {type: 'proton_captcha', token: '...'} — only when token
                                   looks like a combined token (<HV_TOKEN>:<rest>)

    The bare proton_captcha hex-only token is NOT accepted — it's an intermediate value.
    """
    if not isinstance(data, dict):
        return None
    msg_type = data.get('type')
    if msg_type == 'pm_captcha':
        t = data.get('token')
        return t if isinstance(t, str) and t else None
    if msg_type == 'HUMAN_VERIFICATION_SUCCESS':
        payload = data.get('payload', {})
        if isinstance(payload, dict):
            t = payload.get('token')
            return t if isinstance(t, str) and t else None
    if msg_type == 'proton_captcha':
        # Accept if it has the combined format: contains ':' (HV_TOKEN separator)
        t = data.get('token')
        if isinstance(t, str) and t and ':' in t:
            return t
    return None


def on_message(manager, js_result):
    try:
        raw = js_result.get_js_value().to_string()
    except Exception as e:
        log(f"get_js_value error: {e}")
        return

    log(f"MSG: {raw}")
    try:
        data = json.loads(raw)
        if isinstance(data, str):
            data = json.loads(data)
    except Exception:
        data = raw

    token = find_final_token(data) if isinstance(data, dict) else None
    if token:
        log(f"TOKEN FOUND: type={data.get('type')!r}")
        def emit_and_quit():
            print(token, flush=True)
            log("emitted token to stdout, quitting")
            Gtk.main_quit()
            return False
        GLib.timeout_add(500, emit_and_quit)
    else:
        if isinstance(data, dict):
            log(f"MSG ignored (type={data.get('type')!r}) — not a final captcha token")


def on_resource_load(webview, resource, request):
    method = request.get_http_method() or "GET"
    uri = request.get_uri()
    log(f"NETWORK {method} {uri}")


def on_load_changed(webview, load_event):
    names = {0: "STARTED", 1: "REDIRECTED", 2: "COMMITTED", 3: "FINISHED"}
    log(f"LOAD {names.get(int(load_event), load_event)}")


def on_load_failed(webview, load_event, uri, error):
    log(f"LOAD_FAILED uri={uri} error={error}")
    return False


url = sys.argv[1]
log(f"creating GTK window for url={url}")

try:
    win = Gtk.Window()
    win.set_title("Proton Verification")
    win.set_default_size(480, 600)
    win.connect('destroy', Gtk.main_quit)

    wv = WebKit2.WebView()
    wv.connect('resource-load-started', on_resource_load)
    wv.connect('load-changed', on_load_changed)
    wv.connect('load-failed', on_load_failed)

    mgr = wv.get_user_content_manager()
    mgr.connect('script-message-received::captcha', on_message)
    mgr.register_script_message_handler('captcha')
    mgr.add_script(WebKit2.UserScript(
        "window.addEventListener('message',function(e){"
        "if(e.data!=null)window.webkit.messageHandlers.captcha.postMessage(JSON.stringify(e.data));"
        "});",
        WebKit2.UserContentInjectedFrames.ALL_FRAMES,
        WebKit2.UserScriptInjectionTime.START,
        None, None
    ))

    wv.load_uri(url)
    win.add(wv)
    win.show_all()
    log("Gtk.main() starting — window should now be visible")
    Gtk.main()
    log("Gtk.main() returned (window closed)")
except Exception as e:
    import traceback as tb
    log(f"FATAL: {e}\n{tb.format_exc()}")
    sys.exit(1)
