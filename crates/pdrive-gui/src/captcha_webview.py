import sys
import json
import gi
gi.require_version('WebKit2', '4.1')
from gi.repository import WebKit2, Gtk, GLib

HV_TOKEN = sys.argv[2] if len(sys.argv) > 2 else ""

def find_final_token(data):
    """Extract the combined HV token only from pm_captcha or HUMAN_VERIFICATION_SUCCESS.
    These messages carry the final server-accepted combined token, e.g.:
      <HV_TOKEN>:<signature><captcha_hex>
    The earlier proton_captcha message only carries the raw captcha hex and must be ignored."""
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
    return None

def on_message(manager, js_result):
    raw = js_result.get_js_value().to_string()
    print(f"DEBUG msg: {raw}", file=sys.stderr, flush=True)
    try:
        data = json.loads(raw)
        if isinstance(data, str):
            data = json.loads(data)
    except Exception:
        data = raw

    token = find_final_token(data) if isinstance(data, dict) else None
    if token:
        print(f"DEBUG captcha solved, HV_TOKEN={HV_TOKEN!r} ProtonCaptcha={token!r}", file=sys.stderr, flush=True)
        # finalize API call already completed; short delay then emit
        def emit_and_quit():
            print(token, flush=True)
            Gtk.main_quit()
            return False  # don't repeat
        GLib.timeout_add(500, emit_and_quit)

def on_resource_load(webview, resource, request):
    method = request.get_http_method() or "GET"
    uri = request.get_uri()
    print(f"NETWORK {method} {uri}", file=sys.stderr, flush=True)

url = sys.argv[1]
win = Gtk.Window()
win.set_title("Proton Verification")
win.set_default_size(480, 600)
win.connect('destroy', Gtk.main_quit)

wv = WebKit2.WebView()
wv.connect('resource-load-started', on_resource_load)

mgr = wv.get_user_content_manager()
mgr.connect('script-message-received::captcha', on_message)
mgr.register_script_message_handler('captcha')
mgr.add_script(WebKit2.UserScript(
    "window.addEventListener('message',function(e){if(e.data!=null)window.webkit.messageHandlers.captcha.postMessage(JSON.stringify(e.data));});",
    WebKit2.UserContentInjectedFrames.ALL_FRAMES,
    WebKit2.UserScriptInjectionTime.START,
    None, None
))

wv.load_uri(url)
win.add(wv)
win.show_all()
Gtk.main()
