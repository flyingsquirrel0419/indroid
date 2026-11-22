package android.view;

import android.content.Context;
import java.util.ArrayList;

/**
 * Inflates the VM-serialized layout tree. Element: \u0001tag, attribute:
 * \u0002name\u0004value, end of element: \u0003.
 */
public class LayoutInflater {
    private final Context ctx;
    private Factory factory;
    private Factory2 factory2, privateFactory;
    private Filter filter;
    protected LayoutInflater(Context c) { ctx = c; }
    protected LayoutInflater(LayoutInflater original, Context c) { ctx = c; factory = original.factory; factory2 = original.factory2; privateFactory = original.privateFactory; filter = original.filter; }
    /** The context's own inflater (so factories installed by AppCompat persist), like Android. */
    public static LayoutInflater from(Context c) {
        Object o = c.getSystemService(Context.LAYOUT_INFLATER_SERVICE);
        if (o instanceof LayoutInflater) return (LayoutInflater) o;
        return new LayoutInflater(c);
    }
    /** indroid: a plain inflater for contexts that cache one. */
    public static LayoutInflater indroidCreate(Context c) { return new LayoutInflater(c); }
    public Context getContext() { return ctx; }
    public LayoutInflater cloneInContext(Context c) { return new LayoutInflater(this, c); }
    public final Factory getFactory() { return factory; }
    public final Factory2 getFactory2() { return factory2; }
    public void setFactory(Factory f) {
        if (factory != null) throw new IllegalStateException("A factory has already been set on this LayoutInflater");
        if (f == null) throw new NullPointerException("Given factory can not be null");
        factory = f;
    }
    public void setFactory2(Factory2 f) {
        if (factory != null) throw new IllegalStateException("A factory has already been set on this LayoutInflater");
        if (f == null) throw new NullPointerException("Given factory can not be null");
        factory = factory2 = f;
    }
    public void setPrivateFactory(Factory2 f) { privateFactory = f; }
    public Filter getFilter() { return filter; }
    public void setFilter(Filter f) { filter = f; }
    public interface Filter { boolean onLoadClass(Class clazz); }
    public final View createView(String name, String prefix, android.util.AttributeSet attrs) throws ClassNotFoundException, InflateException {
        return createView(ctx, name, prefix, attrs);
    }
    public final View createView(Context viewContext, String name, String prefix, android.util.AttributeSet attrs) throws ClassNotFoundException, InflateException {
        Class<?> c = Class.forName(prefix != null ? prefix + name : name);
        return (View) VMInflate.construct(c, viewContext, attrs instanceof XmlAttrs ? (XmlAttrs) attrs : XmlAttrs.EMPTY);
    }
    protected View onCreateView(String name, android.util.AttributeSet attrs) throws ClassNotFoundException {
        String[] prefixes = { "android.widget.", "android.view.", "android.webkit." };
        for (String pre : prefixes) {
            try { return createView(name, pre, attrs); } catch (ClassNotFoundException e) {}
        }
        throw new ClassNotFoundException(name);
    }
    protected View onCreateView(View parent, String name, android.util.AttributeSet attrs) throws ClassNotFoundException { return onCreateView(name, attrs); }
    public View onCreateView(Context viewContext, View parent, String name, android.util.AttributeSet attrs) throws ClassNotFoundException { return onCreateView(parent, name, attrs); }
    public View inflate(int resource, ViewGroup root) { return inflate(resource, root, root != null); }
    public View inflate(int resource, ViewGroup root, boolean attach) {
        String src = ctx.getResources().getLayoutSource(resource);
        int[] pos = { 0 };
        View v = parse(src, pos, root, attach, true);
        return attach && root != null ? root : v;
    }

    private View parse(String s, int[] pos, ViewGroup parent, boolean attach, boolean top) {
        int p = pos[0];
        if (s.charAt(p) != '\u0001') throw new IllegalStateException("bad layout");
        int e = next(s, p + 1);
        String tag = s.substring(p + 1, e);
        p = e;
        ArrayList<String> names = new ArrayList<String>(), vals = new ArrayList<String>();
        while (p < s.length() && s.charAt(p) == '\u0002') {
            int sep = s.indexOf('\u0004', p);
            int end = next(s, sep + 1);
            names.add(s.substring(p + 1, sep));
            vals.add(s.substring(sep + 1, end));
            p = end;
        }
        XmlAttrs attrs = new XmlAttrs(names.toArray(new String[0]), vals.toArray(new String[0]), ctx);
        pos[0] = p;
        if (tag.equals("merge")) {
            if (parent == null) throw new InflateException("<merge /> can be used only with a valid ViewGroup root and attachToRoot=true");
            parseChildren(s, pos, parent);
            pos[0]++;
            return parent;
        }
        if (tag.equals("include")) {
            int layout = attrs.integer("layout", 0);
            while (s.charAt(pos[0]) != '\u0003') parse(s, pos, null, false, false);
            pos[0]++;
            String inc = ctx.getResources().getLayoutSource(layout);
            if (inc.startsWith("\u0001merge") && (inc.length() == 6 || inc.charAt(6) <= '\u0005')) {
                if (parent == null) throw new InflateException("<merge /> can be used only with a valid ViewGroup root");
                int[] ip = { 0 };
                parse(inc, ip, parent, true, true);
                return parent;
            }
            View v = inflate(layout, null, false);
            int id = attrs.integer("id", View.NO_ID);
            if (id != View.NO_ID) v.setId(id);
            if (parent != null) {
                ViewGroup.LayoutParams lp = attrs.has("layout_width") ? parent.generateLayoutParams(attrs) : v.getLayoutParams();
                parent.addView(v, lp == null ? parent.generateDefaultLayoutParams() : lp);
            }
            return v;
        }
        View v = createViewFromTag(parent, tag, attrs);
        if (parent != null) {
            ViewGroup.LayoutParams lp = parent.generateLayoutParams(attrs);
            if (!top || attach) parent.addView(v, lp);
            else v.setLayoutParams(lp);
        } else if (attrs.has("layout_width")) {
            v.setLayoutParams(new ViewGroup.MarginLayoutParams(ctx, attrs));
        }
        if (v instanceof ViewGroup) parseChildren(s, pos, (ViewGroup) v);
        else while (s.charAt(pos[0]) != '\u0003') parse(s, pos, null, false, false);
        pos[0]++; // \u0003
        if (v instanceof ViewGroup) ((ViewGroup) v).onFinishInflate();
        return v;
    }
    private void parseChildren(String s, int[] pos, ViewGroup parent) {
        while (s.charAt(pos[0]) != '\u0003') parse(s, pos, parent, true, false);
    }
    private static int next(String s, int from) {
        int i = from;
        while (i < s.length()) { char c = s.charAt(i); if (c <= '\u0004') break; i++; }
        return i;
    }
    private View createViewFromTag(View parent, String tag, XmlAttrs attrs) {
        if (tag.equals("view")) tag = attrs.string("class");
        View v = null;
        try {
            if (factory2 != null) v = factory2.onCreateView(parent, tag, ctx, attrs);
            else if (factory != null) v = factory.onCreateView(tag, ctx, attrs);
            if (v == null && privateFactory != null) v = privateFactory.onCreateView(parent, tag, ctx, attrs);
        } catch (RuntimeException e) {
            throw e;
        }
        if (v != null) return v;
        return createViewDefault(tag, attrs);
    }
    protected View createViewDefault(String tag, XmlAttrs attrs) {
        String[] candidates;
        if (tag.equals("view")) tag = attrs.string("class");
        if (tag.indexOf('.') >= 0) candidates = new String[] { mapAndroidx(tag) };
        else candidates = new String[] { "android.widget." + tag, "android.view." + tag, "android.webkit." + tag };
        for (String name : candidates) {
            Class<?> c;
            try { c = Class.forName(name); } catch (ClassNotFoundException e) { continue; }
            return (View) VMInflate.construct(c, ctx, attrs);
        }
        android.util.Log.w("indroid", "Unsupported view <" + tag + ">, using FrameLayout");
        return new android.widget.FrameLayout(ctx, attrs);
    }
    /** Common AndroidX widgets map onto framework equivalents when the app doesn't bundle them. */
    private static String mapAndroidx(String tag) {
        try { Class.forName(tag); return tag; } catch (ClassNotFoundException e) {}
        if (tag.endsWith(".AppCompatTextView") || tag.endsWith(".MaterialTextView")) return "android.widget.TextView";
        if (tag.endsWith(".AppCompatButton") || tag.endsWith(".MaterialButton")) return "android.widget.Button";
        if (tag.endsWith(".AppCompatEditText") || tag.endsWith(".TextInputEditText")) return "android.widget.EditText";
        if (tag.endsWith(".AppCompatImageView")) return "android.widget.ImageView";
        if (tag.endsWith(".NestedScrollView")) return "android.widget.ScrollView";
        if (tag.endsWith(".LinearLayoutCompat")) return "android.widget.LinearLayout";
        if (tag.endsWith(".ConstraintLayout") || tag.endsWith(".CoordinatorLayout") || tag.endsWith(".CardView") || tag.endsWith(".MaterialCardView")) return "android.widget.FrameLayout";
        return tag;
    }
    public interface Factory { View onCreateView(String name, Context context, android.util.AttributeSet attrs); }
    public interface Factory2 extends Factory { View onCreateView(View parent, String name, Context context, android.util.AttributeSet attrs); }
}
