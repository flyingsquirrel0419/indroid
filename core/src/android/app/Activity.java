package android.app;

import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.widget.FrameLayout;

public class Activity extends android.view.ContextThemeWrapper implements Window.Callback, android.view.KeyEvent.Callback, View.OnCreateContextMenuListener, android.content.ComponentCallbacks2, LayoutInflater.Factory2 {
    public static final int RESULT_OK = -1, RESULT_CANCELED = 0, RESULT_FIRST_USER = 1;
    Intent mIntent;
    /** The window's decor view: the root laid out and serialized by ActivityThread. */
    FrameLayout mContent;
    CharSequence mTitle;
    boolean mFinished, mResumed;
    int mResultCode = RESULT_CANCELED;
    Intent mResultData;
    Activity mCaller;
    int mRequestCode = -1;
    private Window window;
    private Application app;

    public Activity() { super(); }
    final void attach(Context base, Application app, Intent intent) {
        attachBaseContext(base);
        this.app = app;
        mIntent = intent;
        int t = ActivityThread.nativeTheme(getClass().getName());
        if (t == 0) t = ActivityThread.nativeTheme("");
        setTheme(t != 0 ? t : INDROID_DEFAULT_THEME);
        window = new Window(this);
        window.setCallback(this);
        mContent = (FrameLayout) window.getDecorView();
    }
    protected void onCreate(Bundle savedInstanceState) {}
    protected void onPostCreate(Bundle savedInstanceState) {}
    protected void onStart() {}
    protected void onRestart() {}
    protected void onResume() {}
    protected void onPostResume() {}
    protected void onPause() {}
    protected void onStop() {}
    protected void onDestroy() {}
    protected void onSaveInstanceState(Bundle out) {}
    protected void onRestoreInstanceState(Bundle in) {}
    protected void onNewIntent(Intent i) {}
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {}
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {}
    public void onBackPressed() { finish(); }
    public void onWindowFocusChanged(boolean hasFocus) {}
    public void onConfigurationChanged(android.content.res.Configuration c) {}
    public void onLowMemory() {}
    public void onUserInteraction() {}
    public boolean onCreateOptionsMenu(android.view.Menu menu) { return false; }
    public boolean onOptionsItemSelected(android.view.MenuItem item) { return false; }
    public boolean onKeyDown(int keyCode, android.view.KeyEvent e) { return false; }

    public void setContentView(int layoutResID) { getWindow().setContentView(layoutResID); onContentChanged(); }
    public void setContentView(View view) {
        ViewGroup.LayoutParams p = view.getLayoutParams();
        getWindow().setContentView(view, p != null ? p : new FrameLayout.LayoutParams(-1, -1));
        onContentChanged();
    }
    public void setContentView(View view, ViewGroup.LayoutParams p) { getWindow().setContentView(view, p); onContentChanged(); }
    public void addContentView(View view, ViewGroup.LayoutParams p) { getWindow().addContentView(view, p); onContentChanged(); }
    public void onContentChanged() {}
    @SuppressWarnings("unchecked")
    public <T extends View> T findViewById(int id) { return (T) getWindow().findViewById(id); }
    @SuppressWarnings("unchecked")
    public final <T extends View> T requireViewById(int id) {
        T v = findViewById(id);
        if (v == null) throw new IllegalArgumentException("ID does not reference a View inside this Activity");
        return v;
    }
    public Intent getIntent() { return mIntent; }
    public void setIntent(Intent i) { mIntent = i; }
    public void setTitle(CharSequence t) { mTitle = t; }
    public void setTitle(int resId) { mTitle = getText(resId); }
    public CharSequence getTitle() { return mTitle; }
    public void finish() { mFinished = true; }
    public void finishAffinity() { mFinished = true; }
    public boolean isFinishing() { return mFinished; }
    public boolean isDestroyed() { return mDestroyed; }
    public void recreate() {}
    public final Application getApplication() { return app; }
    public Window getWindow() { return window; }
    public LayoutInflater getLayoutInflater() { return LayoutInflater.from(this); }
    public android.view.WindowManager getWindowManager() { return null; }
    public final void runOnUiThread(Runnable r) { r.run(); }
    public void startActivity(Intent intent) { ActivityThread.startActivity(this, intent, -1); }
    public void startActivity(Intent intent, Bundle options) { startActivity(intent); }
    public void startActivityForResult(Intent intent, int requestCode) { ActivityThread.startActivity(this, intent, requestCode); }
    public void startActivityForResult(Intent intent, int requestCode, Bundle options) { startActivityForResult(intent, requestCode); }
    public final void setResult(int code) { mResultCode = code; }
    public final void setResult(int code, Intent data) { mResultCode = code; mResultData = data; }
    public android.content.ComponentName getComponentName() { return new android.content.ComponentName(getPackageName(), getClass().getName()); }
    public android.view.MenuInflater getMenuInflater() { return new android.view.MenuInflater(this); }
    public void requestPermissions(String[] p, int code) {
        int[] r = new int[p.length];
        for (int i = 0; i < r.length; i++) r[i] = android.content.pm.PackageManager.PERMISSION_DENIED;
        onRequestPermissionsResult(code, p, r);
    }
    public boolean shouldShowRequestPermissionRationale(String p) { return false; }
    public final boolean requestWindowFeature(int f) { return true; }
    public void setRequestedOrientation(int o) {}
    public void overridePendingTransition(int a, int b) {}
    public void invalidateOptionsMenu() {}
    public ActionBar getActionBar() { return null; }
    public final void showDialog(int id) {}
    public void openOptionsMenu() {}
    public Object getSystemService(String name) {
        if (WINDOW_SERVICE.equals(name)) return getWindowManager();
        return super.getSystemService(name);
    }

    // ---------------------------------------------------------------- lifecycle dispatch
    private final java.util.ArrayList<Application.ActivityLifecycleCallbacks> lifecycleCallbacks = new java.util.ArrayList<Application.ActivityLifecycleCallbacks>();
    public void registerActivityLifecycleCallbacks(Application.ActivityLifecycleCallbacks c) { lifecycleCallbacks.add(c); }
    public void unregisterActivityLifecycleCallbacks(Application.ActivityLifecycleCallbacks c) { lifecycleCallbacks.remove(c); }
    private Object[] callbacks() {
        java.util.ArrayList<Object> all = new java.util.ArrayList<Object>(lifecycleCallbacks);
        if (app != null) synchronized (app.callbacks) { all.addAll(app.callbacks); }
        return all.toArray();
    }
    final void performCreate(Bundle state) {
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPreCreated(this, state);
        onCreate(state);
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityCreated(this, state);
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPostCreated(this, state);
        onPostCreate(state);
    }
    final void performStart() {
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPreStarted(this);
        onStart();
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityStarted(this);
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPostStarted(this);
    }
    final void performResume() {
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPreResumed(this);
        onResume();
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityResumed(this);
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPostResumed(this);
        onPostResume();
        mResumed = true;
        onWindowFocusChanged(true);
    }
    final void performPause() {
        mResumed = false;
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPrePaused(this);
        onPause();
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPaused(this);
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPostPaused(this);
    }
    final void performStop() {
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPreStopped(this);
        onStop();
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityStopped(this);
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPostStopped(this);
    }
    final void performDestroy() {
        mDestroyed = true;
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPreDestroyed(this);
        onDestroy();
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityDestroyed(this);
        for (Object o : callbacks()) ((Application.ActivityLifecycleCallbacks) o).onActivityPostDestroyed(this);
        if (mContent != null) mContent.indroidSetAttached(false);
    }
    boolean mDestroyed;

    // ---------------------------------------------------------------- Window.Callback & input
    public boolean dispatchKeyEvent(android.view.KeyEvent e) {
        onUserInteraction();
        if (e.getKeyCode() == android.view.KeyEvent.KEYCODE_BACK && e.getAction() == android.view.KeyEvent.ACTION_UP) { onBackPressed(); return true; }
        View d = getWindow().getDecorView();
        return d.dispatchKeyEvent(e) || e.dispatch(this, null, this);
    }
    public boolean dispatchKeyShortcutEvent(android.view.KeyEvent e) { return false; }
    public boolean dispatchTouchEvent(android.view.MotionEvent e) {
        if (e.getActionMasked() == android.view.MotionEvent.ACTION_DOWN) onUserInteraction();
        return getWindow().superDispatchTouchEvent(e) || onTouchEvent(e);
    }
    public boolean onTouchEvent(android.view.MotionEvent e) { return false; }
    public boolean dispatchTrackballEvent(android.view.MotionEvent e) { return false; }
    public boolean dispatchGenericMotionEvent(android.view.MotionEvent e) { return false; }
    public boolean dispatchPopulateAccessibilityEvent(android.view.accessibility.AccessibilityEvent e) { return false; }
    public View onCreatePanelView(int featureId) { return null; }
    public boolean onCreatePanelMenu(int featureId, android.view.Menu menu) { return featureId == 0 && onCreateOptionsMenu(menu); }
    public boolean onPreparePanel(int featureId, View view, android.view.Menu menu) { return featureId != 0 || onPrepareOptionsMenu(menu); }
    public boolean onPrepareOptionsMenu(android.view.Menu menu) { return true; }
    public boolean onMenuOpened(int featureId, android.view.Menu menu) { return true; }
    public boolean onMenuItemSelected(int featureId, android.view.MenuItem item) { return featureId == 0 && onOptionsItemSelected(item); }
    public void onPanelClosed(int featureId, android.view.Menu menu) {}
    public void onOptionsMenuClosed(android.view.Menu menu) {}
    public void onWindowAttributesChanged(android.view.WindowManager.LayoutParams p) {}
    public void onAttachedToWindow() {}
    public void onDetachedFromWindow() {}
    public boolean onSearchRequested() { return false; }
    public boolean onSearchRequested(android.view.SearchEvent e) { return false; }
    public android.view.ActionMode onWindowStartingActionMode(android.view.ActionMode.Callback cb) { return null; }
    public android.view.ActionMode onWindowStartingActionMode(android.view.ActionMode.Callback cb, int type) { return null; }
    public void onActionModeStarted(android.view.ActionMode m) {}
    public void onActionModeFinished(android.view.ActionMode m) {}
    public android.view.ActionMode startActionMode(android.view.ActionMode.Callback cb) { return null; }
    public boolean onKeyUp(int code, android.view.KeyEvent e) { return false; }
    public boolean onKeyLongPress(int code, android.view.KeyEvent e) { return false; }
    public boolean onKeyMultiple(int code, int count, android.view.KeyEvent e) { return false; }
    public boolean onKeyShortcut(int code, android.view.KeyEvent e) { return false; }
    public void onCreateContextMenu(android.view.ContextMenu menu, View v, android.view.ContextMenu.ContextMenuInfo info) {}
    public boolean onContextItemSelected(android.view.MenuItem item) { return false; }
    public void registerForContextMenu(View v) {}
    public void unregisterForContextMenu(View v) {}
    public View onCreateView(String name, Context c, android.util.AttributeSet attrs) { return null; }
    public View onCreateView(View parent, String name, Context c, android.util.AttributeSet attrs) { return onCreateView(name, c, attrs); }
    public void onTrimMemory(int level) {}
    public void onUserLeaveHint() {}
    public void onTitleChanged(CharSequence title, int color) {}
    public void onMultiWindowModeChanged(boolean b, android.content.res.Configuration c) {}
    public void onMultiWindowModeChanged(boolean b) {}
    public void onPictureInPictureModeChanged(boolean b, android.content.res.Configuration c) {}
    public void onPictureInPictureModeChanged(boolean b) {}
    public boolean isInMultiWindowMode() { return false; }
    public boolean isInPictureInPictureMode() { return false; }
    public void closeOptionsMenu() {}
    public void reportFullyDrawn() {}
    public boolean isChangingConfigurations() { return false; }
    public boolean isChild() { return false; }
    public final Activity getParent() { return null; }
    public Object getLastNonConfigurationInstance() { return null; }
    public Object onRetainNonConfigurationInstance() { return null; }
    public FragmentManager getFragmentManager() { if (fragments == null) fragments = new FragmentManager(); return fragments; }
    private FragmentManager fragments;
    private final android.window.OnBackInvokedDispatcher backDispatcher = new android.window.OnBackInvokedDispatcher() {
        public void registerOnBackInvokedCallback(int priority, android.window.OnBackInvokedCallback cb) {}
        public void unregisterOnBackInvokedCallback(android.window.OnBackInvokedCallback cb) {}
    };
    public android.window.OnBackInvokedDispatcher getOnBackInvokedDispatcher() { return backDispatcher; }
    public Intent getParentActivityIntent() { return null; }
    public boolean navigateUpTo(Intent i) { finish(); return true; }
    public boolean onNavigateUp() { finish(); return true; }
    public boolean shouldUpRecreateTask(Intent i) { return false; }
    public android.view.DragAndDropPermissions requestDragAndDropPermissions(android.view.DragEvent e) { return null; }
    public void startIntentSenderForResult(android.content.IntentSender s, int code, Intent fill, int mask, int values, int extra) {}
    public void startIntentSenderForResult(android.content.IntentSender s, int code, Intent fill, int mask, int values, int extra, Bundle options) {}
    public void dump(String prefix, java.io.FileDescriptor fd, java.io.PrintWriter w, String[] args) {}
    public boolean isTaskRoot() { return true; }
    public int getTaskId() { return 1; }
    public boolean moveTaskToBack(boolean b) { return false; }
    public void setFinishOnTouchOutside(boolean b) {}
    public void setShowWhenLocked(boolean b) {}
    public void setTurnScreenOn(boolean b) {}
    public android.view.View getCurrentFocus() { return getWindow().getCurrentFocus(); }
    public boolean hasWindowFocus() { return mResumed; }
    public void takeKeyEvents(boolean b) {}
    public void setVolumeControlStream(int s) {}
    public void enterPictureInPictureMode() {}
    public void setDefaultKeyMode(int m) {}
    public final CharSequence getTitleCondensed() { return mTitle; }
    public void setTitleColor(int c) {}
    public final boolean isVoiceInteraction() { return false; }
    public void finishAfterTransition() { finish(); }
    public void postponeEnterTransition() {}
    public void startPostponedEnterTransition() {}
    public void setEnterSharedElementCallback(SharedElementCallback c) {}
    public void setExitSharedElementCallback(SharedElementCallback c) {}
}
