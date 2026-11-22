package java.lang;

import java.io.PrintStream;

public final class System {
    private System() {}
    public static final PrintStream out = new PrintStream(1);
    public static final PrintStream err = new PrintStream(2);
    public static final java.io.InputStream in = null;
    public static native void arraycopy(Object src, int sp, Object dst, int dp, int len);
    public static native long currentTimeMillis();
    public static native long nanoTime();
    public static native int identityHashCode(Object o);
    public static String getProperty(String k) { return getProperty(k, null); }
    public static String getProperty(String k, String def) {
        if ("line.separator".equals(k)) return "\n";
        if ("java.vm.name".equals(k)) return "indroid";
        if ("os.name".equals(k)) return "Linux";
        if ("file.separator".equals(k)) return "/";
        if ("http.agent".equals(k)) return "indroid";
        return def;
    }
    public static String getenv(String k) { return null; }
    public static String lineSeparator() { return "\n"; }
    public static native void exit(int code);
    public static void gc() {}
    public static void runFinalization() {}
    public static void loadLibrary(String lib) { throw new UnsatisfiedLinkError("native libraries are not supported on indroid: " + lib); }
    public static void load(String lib) { loadLibrary(lib); }
}
