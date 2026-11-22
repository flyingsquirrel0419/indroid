package android.app;

import android.content.Context;
import android.content.DialogInterface;
import android.view.View;

public class Dialog implements DialogInterface {
    final Context ctx;
    CharSequence title;
    View content;
    boolean cancelable = true, showing;
    OnCancelListener onCancel;
    OnDismissListener onDismiss;
    public Dialog(Context c) { ctx = c; }
    public Dialog(Context c, int theme) { ctx = c; }
    public final Context getContext() { return ctx; }
    public void setTitle(CharSequence t) { title = t; }
    public void setTitle(int r) { title = ctx.getText(r); }
    public void setContentView(View v) { content = v; }
    public void setContentView(int layout) { content = android.view.LayoutInflater.from(ctx).inflate(layout, null); }
    @SuppressWarnings("unchecked")
    public <T extends View> T findViewById(int id) { return content == null ? null : (T) content.findViewById(id); }
    public void setCancelable(boolean c) { cancelable = c; }
    public void setCanceledOnTouchOutside(boolean c) {}
    public void setOnCancelListener(OnCancelListener l) { onCancel = l; }
    public void setOnDismissListener(OnDismissListener l) { onDismiss = l; }
    public void show() { if(!created) { onCreate(null); created=true; } showing = true; onStart(); ActivityThread.showDialog(this); }
    public boolean isShowing() { return showing; }
    public void dismiss() {
        if (!showing) return;
        showing = false;
        onStop();
        ActivityThread.dismissDialog(this);
        if (onDismiss != null) onDismiss.onDismiss(this);
    }
    public void cancel() { if (onCancel != null) onCancel.onCancel(this); dismiss(); }
    public void hide() { dismiss(); }
    public android.view.Window getWindow() { return new android.view.Window(); }
    /** indroid: host button press (-1 positive, -2 negative, -3 neutral, >= 0 list item). */
    void indroidButton(int which) { dismiss(); }
    void indroidJson(StringBuilder sb) {
        sb.append("{\"title\":"); android.view.Json.str(sb, title);
        if (content != null) { sb.append(",\"content\":"); content.indroidJson(sb); }
        sb.append(",\"cancelable\":").append(cancelable);
        sb.append('}');
    }
    private OnKeyListener onKey; private Activity owner; private boolean created;
    public void setContentView(View v,android.view.ViewGroup.LayoutParams params) { content=v; v.setLayoutParams(params); } public void addContentView(View v,android.view.ViewGroup.LayoutParams params) { if(content instanceof android.view.ViewGroup) ((android.view.ViewGroup)content).addView(v,params); else setContentView(v,params); }
    protected void onCreate(android.os.Bundle state) {} protected void onStart() {} protected void onStop() {} public void onBackPressed() { if(cancelable) cancel(); } public boolean onKeyDown(int keyCode,android.view.KeyEvent event) { return false; } public boolean onKeyUp(int keyCode,android.view.KeyEvent event) { return false; } public boolean onTouchEvent(android.view.MotionEvent event) { return false; } public boolean dispatchKeyEvent(android.view.KeyEvent event) { return onKey!=null && onKey.onKey(this,event.getKeyCode(),event); } public void setOnKeyListener(OnKeyListener listener) { onKey=listener; } public boolean requestWindowFeature(int feature) { return true; } public void setOwnerActivity(Activity activity) { owner=activity; } public Activity getOwnerActivity() { return owner; } public android.os.Bundle onSaveInstanceState() { return new android.os.Bundle(); } public void onRestoreInstanceState(android.os.Bundle state) {}

 public android.window.OnBackInvokedDispatcher getOnBackInvokedDispatcher() { return null; }
}
