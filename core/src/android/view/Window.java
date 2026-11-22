package android.view;

import android.content.Context;
import android.graphics.drawable.Drawable;
import android.widget.FrameLayout;

/**
 * The activity window: a decor view whose child with id android.R.id.content
 * holds the activity's content, like PhoneWindow.
 */
public class Window {
    public static final int FEATURE_OPTIONS_PANEL = 0, FEATURE_NO_TITLE = 1, FEATURE_PROGRESS = 2, FEATURE_LEFT_ICON = 3,
        FEATURE_RIGHT_ICON = 4, FEATURE_INDETERMINATE_PROGRESS = 5, FEATURE_CONTEXT_MENU = 6, FEATURE_CUSTOM_TITLE = 7,
        FEATURE_ACTION_BAR = 8, FEATURE_ACTION_BAR_OVERLAY = 9, FEATURE_ACTION_MODE_OVERLAY = 10, FEATURE_SWIPE_TO_DISMISS = 11,
        FEATURE_CONTENT_TRANSITIONS = 12, FEATURE_ACTIVITY_TRANSITIONS = 13;
    public static final int ID_ANDROID_CONTENT = 0x01020002;
    public static final int DECOR_CAPTION_SHADE_AUTO = 0, PROGRESS_VISIBILITY_ON = -1, PROGRESS_VISIBILITY_OFF = -2;
    public static final String NAVIGATION_BAR_BACKGROUND_TRANSITION_NAME = "android:navigation:background", STATUS_BAR_BACKGROUND_TRANSITION_NAME = "android:status:background";

    /** Activity hooks the window calls (Activity implements this). */
    public interface Callback {
        boolean dispatchKeyEvent(KeyEvent event);
        boolean dispatchKeyShortcutEvent(KeyEvent event);
        boolean dispatchTouchEvent(MotionEvent event);
        boolean dispatchTrackballEvent(MotionEvent event);
        boolean dispatchGenericMotionEvent(MotionEvent event);
        boolean dispatchPopulateAccessibilityEvent(android.view.accessibility.AccessibilityEvent event);
        View onCreatePanelView(int featureId);
        boolean onCreatePanelMenu(int featureId, Menu menu);
        boolean onPreparePanel(int featureId, View view, Menu menu);
        boolean onMenuOpened(int featureId, Menu menu);
        boolean onMenuItemSelected(int featureId, MenuItem item);
        void onWindowAttributesChanged(WindowManager.LayoutParams attrs);
        void onContentChanged();
        void onWindowFocusChanged(boolean hasFocus);
        void onAttachedToWindow();
        void onDetachedFromWindow();
        void onPanelClosed(int featureId, Menu menu);
        boolean onSearchRequested();
        boolean onSearchRequested(SearchEvent searchEvent);
        ActionMode onWindowStartingActionMode(ActionMode.Callback callback);
        ActionMode onWindowStartingActionMode(ActionMode.Callback callback, int type);
        void onActionModeStarted(ActionMode mode);
        void onActionModeFinished(ActionMode mode);
        default void onProvideKeyboardShortcuts(java.util.List data, Menu menu, int deviceId) {}
        default void onPointerCaptureChanged(boolean hasCapture) {}
    }
    private Callback callback;
    public void setCallback(Callback c) { callback = c; }
    public final Callback getCallback() { return callback; }

    private final Context context;
    private FrameLayout decor;
    private FrameLayout contentParent;
    private int features;
    private int statusBarColor, navigationBarColor = 0xff000000;
    private boolean decorFitsSystemWindows = true;
    private WindowManager.LayoutParams attrs = new WindowManager.LayoutParams();
    private CharSequence title;

    public Window(Context context) { this.context = context; }
    /** indroid: legacy no-arg constructor. */
    public Window() { this(null); }
    public final Context getContext() { return context; }

    private void installDecor() {
        if (decor != null) return;
        decor = new DecorView(context);
        decor.setLayoutParams(new ViewGroup.LayoutParams(-1, -1));
        contentParent = new FrameLayout(context);
        contentParent.setId(ID_ANDROID_CONTENT);
        decor.addView(contentParent, new FrameLayout.LayoutParams(-1, -1));
        if (context != null) {
            try {
                android.content.res.TypedArray a = context.obtainStyledAttributes(new int[] { 0x01010054 /* windowBackground */ });
                Drawable bg = a.getDrawable(0);
                a.recycle();
                if (bg != null) decor.setBackground(bg);
            } catch (RuntimeException e) {} catch (LinkageError e) {}
        }
    }
    public View getDecorView() { installDecor(); return decor; }
    public final View peekDecorView() { return decor; }
    /** indroid: the view group holding the activity content (id android.R.id.content at first). */
    public ViewGroup indroidContentParent() { installDecor(); return contentParent; }
    @SuppressWarnings("unchecked")
    public <T extends View> T findViewById(int id) { return (T) getDecorView().findViewById(id); }
    public final <T extends View> T requireViewById(int id) {
        T v = findViewById(id);
        if (v == null) throw new IllegalArgumentException("ID does not reference a View inside this Window");
        return v;
    }
    public void setContentView(int layoutResID) {
        installDecor();
        contentParent.removeAllViews();
        LayoutInflater.from(context).inflate(layoutResID, contentParent, true);
    }
    public void setContentView(View view) { setContentView(view, new ViewGroup.LayoutParams(-1, -1)); }
    public void setContentView(View view, ViewGroup.LayoutParams p) {
        installDecor();
        contentParent.removeAllViews();
        contentParent.addView(view, p);
    }
    public void addContentView(View view, ViewGroup.LayoutParams p) { installDecor(); contentParent.addView(view, p); }
    public View getCurrentFocus() { return null; }
    public LayoutInflater getLayoutInflater() { return LayoutInflater.from(context); }
    public boolean requestFeature(int f) { features |= 1 << f; return true; }
    public boolean hasFeature(int f) { return (features & (1 << f)) != 0; }
    protected final int getFeatures() { return features; }
    public void setFlags(int f, int m) { attrs.flags = (attrs.flags & ~m) | (f & m); }
    public void addFlags(int f) { setFlags(f, f); }
    public void clearFlags(int f) { setFlags(0, f); }
    public void setStatusBarColor(int c) { statusBarColor = c; }
    public int getStatusBarColor() { return statusBarColor; }
    public void setNavigationBarColor(int c) { navigationBarColor = c; }
    public int getNavigationBarColor() { return navigationBarColor; }
    public void setNavigationBarDividerColor(int c) {}
    public void setStatusBarContrastEnforced(boolean b) {}
    public void setNavigationBarContrastEnforced(boolean b) {}
    public boolean isStatusBarContrastEnforced() { return false; }
    public boolean isNavigationBarContrastEnforced() { return false; }
    public void setDecorFitsSystemWindows(boolean b) { decorFitsSystemWindows = b; }
    private final WindowInsetsController insetsController = new WindowInsetsController() {
        private int appearance, behavior;
        public void show(int types) {}
        public void hide(int types) {}
        public void setSystemBarsAppearance(int a, int mask) { appearance = (appearance & ~mask) | (a & mask); }
        public int getSystemBarsAppearance() { return appearance; }
        public void setSystemBarsBehavior(int b) { behavior = b; }
        public int getSystemBarsBehavior() { return behavior; }
    };
    public WindowInsetsController getInsetsController() { return insetsController; }
    public boolean indroidDecorFitsSystemWindows() { return decorFitsSystemWindows; }
    public void setSoftInputMode(int m) { attrs.softInputMode = m; }
    public void setBackgroundDrawable(Drawable d) { installDecor(); decor.setBackground(d); }
    public void setBackgroundDrawableResource(int r) { if (context != null) setBackgroundDrawable(context.getDrawable(r)); }
    public void setLayout(int w, int h) { attrs.width = w; attrs.height = h; }
    public void setGravity(int g) {}
    public void setDimAmount(float a) {}
    public void setWindowAnimations(int r) {}
    public void setElevation(float e) {}
    public void setTitle(CharSequence t) { title = t; }
    public void setTitleColor(int c) {}
    public void setType(int t) {}
    public void setFormat(int f) {}
    public void setSustainedPerformanceMode(boolean b) {}
    public void setAllowEnterTransitionOverlap(boolean b) {}
    public void setAllowReturnTransitionOverlap(boolean b) {}
    public void setSharedElementsUseOverlay(boolean b) {}
    public void setUiOptions(int o) {}
    public void setHideOverlayWindows(boolean b) {}
    public void setCloseOnTouchOutside(boolean b) {}
    public boolean isFloating() { return false; }
    public boolean isActive() { return true; }
    public void makeActive() {}
    public void takeKeyEvents(boolean b) {}
    public void injectInputEvent(Object e) {}
    public boolean superDispatchKeyEvent(KeyEvent e) { return false; }
    public boolean superDispatchKeyShortcutEvent(KeyEvent e) { return false; }
    public boolean superDispatchTouchEvent(MotionEvent e) { return decor != null && decor.dispatchTouchEvent(e); }
    public boolean superDispatchTrackballEvent(MotionEvent e) { return false; }
    public boolean superDispatchGenericMotionEvent(MotionEvent e) { return false; }
    public WindowManager.LayoutParams getAttributes() { return attrs; }
    public void setAttributes(WindowManager.LayoutParams p) { if (p != null) attrs = p; }
    public WindowManager getWindowManager() { return null; }
    public void setWindowManager(WindowManager wm, android.os.IBinder token, String name) {}
    public void setLocalFocus(boolean hasFocus, boolean inTouchMode) {}
    public void closeAllPanels() {}
    public void closePanel(int id) {}
    public void openPanel(int id, KeyEvent e) {}
    public void invalidatePanelMenu(int id) {}
    public void togglePanel(int id, KeyEvent e) {}
    public void setFeatureInt(int f, int v) {}
    public void setFeatureDrawableResource(int f, int r) {}
    public void restoreHierarchyState(android.os.Bundle b) {}
    public android.os.Bundle saveHierarchyState() { return new android.os.Bundle(); }

    /** Root of the window's view hierarchy. */
    static final class DecorView extends FrameLayout {
        DecorView(Context c) { super(c); }
    }
}
