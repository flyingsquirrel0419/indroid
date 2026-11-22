package android.app;

import android.content.Intent;
import android.os.Looper;
import android.view.Json;
import android.view.View;
import android.view.VMViews;
import java.util.ArrayList;

/**
 * indroid host entry points. The Rust side calls these static methods; each
 * returns the UI JSON described in docs/PLAN.md.
 */
public final class ActivityThread {
    static String packageName = "";
    static String appLabel = "";
    private static Application app;
    private static ContextImpl base;
    private static final ArrayList<Activity> stack = new ArrayList<Activity>();
    private static final ArrayList<String> toasts = new ArrayList<String>();
    private static final ArrayList<Dialog> dialogs = new ArrayList<Dialog>();
    private static String error;
    private static boolean allFinished;

    private ActivityThread() {}
    /** android:theme from the manifest for an activity class ("" = application); 0 if none. */
    static native int nativeTheme(String activityClass);

    public static String launch(String pkg, String label, String activityClass, String appClass) {
        packageName = pkg;
        appLabel = label;
        base = new ContextImpl();
        try {
            app = appClass != null && appClass.length() > 0 ? (Application) Class.forName(appClass).newInstance() : new Application();
            app.attach(base);
            base.app = app;
            app.onCreate();
            Intent i = new Intent(Intent.ACTION_MAIN);
            i.setClassName(pkg, activityClass);
            i.addCategory(Intent.CATEGORY_LAUNCHER);
            startActivity(null, i, -1);
        } catch (Throwable t) { fail(t); }
        return settle();
    }

    static void startActivity(Activity caller, Intent intent, int requestCode) {
        if (intent.getComponent() == null) {
            toast("No app can handle " + (intent.getAction() == null ? "this action" : intent.getAction()));
            return;
        }
        String cls = intent.getComponent().getClassName();
        Activity a;
        try { a = (Activity) Class.forName(cls).newInstance(); }
        catch (Throwable t) { throw new RuntimeException("Unable to instantiate activity " + cls, t); }
        a.attach(base, app, intent);
        a.mCaller = caller;
        a.mRequestCode = requestCode;
        a.mTitle = appLabel;
        Activity top = top();
        if (top != null && top.mResumed) top.performPause();
        stack.add(a);
        a.performCreate(null);
        if (!a.mFinished) {
            a.performStart();
            a.performResume();
            if (top != null) top.performStop();
        }
    }

    private static Activity top() { return stack.isEmpty() ? null : stack.get(stack.size() - 1); }

    /** Pops finished activities, resumes the new top, runs queued work. */
    private static String settle() {
        try {
            for (int guard = 0; guard < 100; guard++) {
                Looper.drain();
                boolean changed = false;
                for (int i = stack.size() - 1; i >= 0; i--) {
                    Activity a = stack.get(i);
                    if (!a.mFinished) continue;
                    changed = true;
                    stack.remove(i);
                    if (a.mResumed) a.performPause();
                    a.performStop();
                    a.performDestroy();
                    if (a.mCaller != null && a.mRequestCode >= 0 && stack.contains(a.mCaller)) a.mCaller.onActivityResult(a.mRequestCode, a.mResultCode, a.mResultData);
                }
                Activity t = top();
                if (t != null && !t.mResumed && !t.mFinished) {
                    changed = true;
                    t.onRestart();
                    t.performStart();
                    t.performResume();
                }
                if (!changed) break;
            }
            if (stack.isEmpty()) allFinished = true;
        } catch (Throwable t) { fail(t); }
        return dump(Looper.drain());
    }

    private static void fail(Throwable t) {
        error = t.toString();
        android.util.Log.e("AndroidRuntime", "FATAL EXCEPTION: main", t);
    }

    public static String click(int uid) {
        try {
            if (!dialogs.isEmpty() && uid < 0 && uid >= -1000) {
                dialogs.get(dialogs.size() - 1).indroidButton(uid <= -100 ? -uid - 100 : uid);
            } else {
                View v = find(uid);
                if (v != null && v.isEnabled()) v.performClick();
            }
        } catch (Throwable t) { fail(t); }
        return settle();
    }

    public static String setSize(int uid,int w,int h) { try { View v=find(uid); if(v!=null) v.indroidSetSize(w,h); } catch(Throwable t) { fail(t); } return settle(); }
    private static long touchDown;
    public static String touch(int uid,int action,float x,float y) { try { View v=find(uid); if(v!=null) { long now=android.os.SystemClock.uptimeMillis(); if(action==0) touchDown=now; v.dispatchTouchEvent(android.view.MotionEvent.obtain(touchDown,now,action,x,y,0)); } } catch(Throwable t) { fail(t); } return settle(); }

    /** Touch in window coordinates, dispatched through the view tree like Android. */
    public static String touchWindow(int action, float x, float y) {
        try {
            Activity t = top();
            if (t != null) {
                long now = android.os.SystemClock.uptimeMillis();
                if (action == 0) touchDown = now;
                android.view.MotionEvent e = android.view.MotionEvent.obtain(touchDown, now, action, x, y, 0);
                t.mContent.dispatchTouchEvent(e);
            }
        } catch (Throwable e) { fail(e); }
        return settle();
    }

    /** The host scrolled a native scroll view: keep Java's scroll offset in sync (no UI dump). */
    public static String scroll(int uid, int x, int y) {
        try { View v = find(uid); if (v != null) v.scrollTo(x, y); } catch (Throwable e) { fail(e); }
        return "{}";
    }

    public static String longClick(int uid) {
        try { View v = find(uid); if (v != null) v.performLongClick(); } catch (Throwable t) { fail(t); }
        return settle();
    }

    public static String setText(int uid, String text) {
        try {
            View v = find(uid);
            if (v instanceof android.widget.TextView) ((android.widget.TextView) v).indroidUserText(text);
        } catch (Throwable t) { fail(t); }
        return settle();
    }

    public static String editorAction(int uid) {
        try { View v = find(uid); if (v instanceof android.widget.TextView) ((android.widget.TextView) v).indroidEditorAction(); } catch (Throwable t) { fail(t); }
        return settle();
    }

    public static String setProgress(int uid, int p) {
        try { View v = find(uid); if (v instanceof android.widget.SeekBar) ((android.widget.SeekBar) v).indroidUserProgress(p); } catch (Throwable t) { fail(t); }
        return settle();
    }

    public static String back() {
        try {
            if (!dialogs.isEmpty()) {
                Dialog d = dialogs.get(dialogs.size() - 1);
                if (d.cancelable) d.cancel();
            } else {
                Activity t = top();
                if (t != null) t.onBackPressed();
            }
        } catch (Throwable t) { fail(t); }
        return settle();
    }

    /** Timer callback from the host for delayed Handler messages. */
    public static String tick() { return settle(); }

    public static String ui() { return dump(Looper.drain()); }

    private static View find(int uid) {
        for (int i = dialogs.size() - 1; i >= 0; i--) {
            View c = dialogs.get(i).content;
            if (c != null) { View v = VMViews.byUid(c, uid); if (v != null) return v; }
        }
        Activity t = top();
        return t == null ? null : VMViews.byUid(t.mContent, uid);
    }

    public static void toast(String s) { toasts.add(s); }
    static void showDialog(Dialog d) { if (!dialogs.contains(d)) dialogs.add(d); }
    static void dismissDialog(Dialog d) { dialogs.remove(d); }

    /** Content area in points (host reports it; safe areas already removed). */
    static int winW = 390, winH = 760;

    public static String setWindowSize(int w, int h) {
        if (w > 0 && h > 0 && (w != winW || h != winH)) {
            winW = w; winH = h;
            android.util.DisplayMetrics dm = base == null ? null : base.getResources().getDisplayMetrics();
            if (dm != null) { dm.widthPixels = w; dm.heightPixels = h; }
            Activity t = top();
            if (t != null) t.mContent.requestLayout();
        }
        return settle();
    }

    /** Android's measure + layout pass on the visible windows. */
    private static void layoutWindows(Activity t) {
        if (t != null) {
            View root = t.mContent;
            root.indroidSetAttached(true);
            root.measure(View.MeasureSpec.makeMeasureSpec(winW, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(winH, View.MeasureSpec.EXACTLY));
            root.layout(0, 0, root.getMeasuredWidth(), root.getMeasuredHeight());
        }
        for (int i = 0; i < dialogs.size(); i++) {
            View c = dialogs.get(i).content;
            if (c == null) continue;
            if (c.getLayoutParams() == null) c.setLayoutParams(new android.view.ViewGroup.LayoutParams(-1, -2));
            c.indroidSetAttached(true);
            c.measure(View.MeasureSpec.makeMeasureSpec(Math.max(0, winW - 80), View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(winH * 2 / 3, View.MeasureSpec.AT_MOST));
            c.layout(0, 0, c.getMeasuredWidth(), c.getMeasuredHeight());
        }
        View.indroidTakeLayoutRequest();
    }

    private static String dump(long nextMs) {
        StringBuilder sb = new StringBuilder(1024);
        Activity t = top();
        try { layoutWindows(t); } catch (Throwable e) { fail(e); }
        sb.append("{\"title\":");
        Json.str(sb, t == null ? appLabel : (t.mTitle == null ? appLabel : t.mTitle));
        sb.append(",\"finished\":").append(allFinished || t == null);
        sb.append(",\"error\":");
        Json.str(sb, error);
        error = null;
        int nextPos = sb.length();
        sb.append(",\"depth\":").append(stack.size());
        if (t != null) { sb.append(",\"root\":"); t.mContent.indroidJson(sb); }
        if (!toasts.isEmpty()) {
            sb.append(",\"toasts\":[");
            for (int i = 0; i < toasts.size(); i++) { if (i > 0) sb.append(','); Json.str(sb, toasts.get(i)); }
            sb.append(']');
            toasts.clear();
        }
        if (!dialogs.isEmpty()) { sb.append(",\"dialog\":"); dialogs.get(dialogs.size() - 1).indroidJson(sb); }
        boolean dirty = View.indroidTakeDirty();
        if (dirty && (nextMs < 0 || nextMs > 16)) nextMs = 16;
        sb.insert(nextPos, ",\"next\":" + nextMs);
        sb.append('}');
        return sb.toString();
    }
}
