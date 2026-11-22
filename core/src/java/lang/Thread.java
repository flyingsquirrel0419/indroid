package java.lang;

/**
 * indroid runs Java code on a single interpreter thread. Thread.start() hands
 * the runnable to the VM which runs it cooperatively (after the current event
 * finishes, see VMThread). sleep() does not block.
 */
public class Thread implements Runnable {
    public static final int MIN_PRIORITY = 1, NORM_PRIORITY = 5, MAX_PRIORITY = 10;
    private static Thread main;
    private static int counter;
    private static Thread current;
    private Runnable target;
    private String name;
    private boolean daemon;
    private int priority = NORM_PRIORITY;
    private boolean started, interrupted;
    private final long id;
    private UncaughtExceptionHandler handler;
    private static UncaughtExceptionHandler defaultHandler;
    java.util.HashMap<ThreadLocal<?>, Object> locals;

    public interface UncaughtExceptionHandler { void uncaughtException(Thread t, Throwable e); }

    public Thread() { this((Runnable) null, (String) null); }
    public Thread(Runnable r) { this(r, null); }
    public Thread(Runnable r, String n) { target = r; id = ++counter; name = n != null ? n : "Thread-" + id; }
    public Thread(String n) { this((Runnable) null, n); }
    public Thread(ThreadGroup g, Runnable r) { this(r, null); }
    public Thread(ThreadGroup g, Runnable r, String n) { this(r, n); }
    public Thread(ThreadGroup g, Runnable r, String n, long stack) { this(r, n); }
    public Thread(ThreadGroup g, String n) { this((Runnable) null, n); }

    public static Thread currentThread() {
        if (current != null) return current;
        if (main == null) main = new Thread((Runnable) null, "main");
        return main;
    }
    public void run() { if (target != null) target.run(); }
    public synchronized void start() {
        if (started) throw new IllegalThreadStateException();
        started = true;
        VMThread.enqueue(this);
    }
    /** Called by the VM scheduler. */
    final void runAsThread() {
        Thread prev = current;
        current = this;
        try { run(); }
        catch (Throwable t) {
            UncaughtExceptionHandler h = handler != null ? handler : defaultHandler;
            if (h != null) h.uncaughtException(this, t);
            else { System.err.print("Exception in thread \"" + name + "\" "); t.printStackTrace(); }
        } finally { current = prev; }
    }
    public static void sleep(long ms) throws InterruptedException {}
    public static void sleep(long ms, int ns) throws InterruptedException {}
    public static void yield() {}
    public static boolean interrupted() { Thread t = currentThread(); boolean r = t.interrupted; t.interrupted = false; return r; }
    public boolean isInterrupted() { return interrupted; }
    public void interrupt() { interrupted = true; }
    public final void join() throws InterruptedException { VMThread.runPending(); }
    public final void join(long ms) throws InterruptedException { join(); }
    public final boolean isAlive() { return started && VMThread.isPending(this); }
    public final boolean isDaemon() { return daemon; }
    public final void setDaemon(boolean d) { daemon = d; }
    public final String getName() { return name; }
    public final void setName(String n) { name = n; }
    public final int getPriority() { return priority; }
    public final void setPriority(int p) { priority = p; }
    public long getId() { return id; }
    public ClassLoader getContextClassLoader() { return ClassLoader.getSystemClassLoader(); }
    public void setContextClassLoader(ClassLoader l) {}
    public StackTraceElement[] getStackTrace() { return new Throwable().getStackTrace(); }
    public ThreadGroup getThreadGroup() { return null; }
    public void setUncaughtExceptionHandler(UncaughtExceptionHandler h) { handler = h; }
    public UncaughtExceptionHandler getUncaughtExceptionHandler() { return handler; }
    public static void setDefaultUncaughtExceptionHandler(UncaughtExceptionHandler h) { defaultHandler = h; }
    public static UncaughtExceptionHandler getDefaultUncaughtExceptionHandler() { return defaultHandler; }
    public String toString() { return "Thread[" + name + "," + priority + ",main]"; }
}
