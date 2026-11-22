package android.widget;
import android.content.Context;
import android.util.AttributeSet;
import android.view.Json;
import android.view.View;
import android.view.XmlAttrs;
public class ProgressBar extends View {
    int progress, max = 100;
    private boolean indeterminate = true;
    public ProgressBar(Context c) { this(c, null); }
    public ProgressBar(Context c, AttributeSet a) { this(c, a, 16842871 /*progressBarStyle*/); }
    public ProgressBar(Context c, AttributeSet a, int s) {
        super(c, a, s);
        if (a instanceof XmlAttrs) {
            XmlAttrs x = (XmlAttrs) a;
            max = x.integer("max", 100); progress = x.integer("progress", 0);
            indeterminate = x.bool("indeterminate", !x.has("progress") && !(this instanceof SeekBar));
        }
    }
    public synchronized void setProgress(int p) { progress = Math.max(0, Math.min(max, p)); indeterminate = false; }
    public void setProgress(int p, boolean animate) { setProgress(p); }
    public synchronized int getProgress() { return progress; }
    public synchronized void setMax(int m) { max = m; }
    public synchronized int getMax() { return max; }
    public synchronized void setIndeterminate(boolean b) { indeterminate = b; }
    public synchronized boolean isIndeterminate() { return indeterminate; }
    public synchronized void incrementProgressBy(int d) { setProgress(progress + d); }
    protected void onMeasure(int ws, int hs) {
        boolean bar = !indeterminate || this instanceof SeekBar;
        int dw = bar ? 200 : 48, dh = this instanceof SeekBar ? 32 : bar ? 16 : 48;
        setMeasuredDimension(resolveSize(Math.max(dw + getPaddingLeft() + getPaddingRight(), getSuggestedMinimumWidth()), ws),
                             resolveSize(Math.max(dh + getPaddingTop() + getPaddingBottom(), getSuggestedMinimumHeight()), hs));
    }
    protected String indroidType() { return "ProgressBar"; }
    protected void indroidProps(StringBuilder sb) {
        Json.key(sb, "progress"); sb.append(progress);
        Json.key(sb, "max"); sb.append(max);
        Json.key(sb, "indeterminate"); sb.append(indeterminate);
    }
    private android.graphics.drawable.Drawable progressDrawable,indeterminateDrawable; private android.content.res.ColorStateList progressTint,indeterminateTint;
    public ProgressBar(Context c,AttributeSet a,int s,int r) { this(c,a,s); }
    public void setProgressDrawable(android.graphics.drawable.Drawable d) { progressDrawable=d; } public android.graphics.drawable.Drawable getProgressDrawable() { return progressDrawable; } public void setIndeterminateDrawable(android.graphics.drawable.Drawable d) { indeterminateDrawable=d; } public android.graphics.drawable.Drawable getIndeterminateDrawable() { return indeterminateDrawable; }
    public void setProgressTintList(android.content.res.ColorStateList t) { progressTint=t; } public android.content.res.ColorStateList getProgressTintList() { return progressTint; } public void setIndeterminateTintList(android.content.res.ColorStateList t) { indeterminateTint=t; } public android.content.res.ColorStateList getIndeterminateTintList() { return indeterminateTint; }

}
