package android.widget;
import android.content.Context;
import android.util.AttributeSet;
public class SeekBar extends AbsSeekBar {
    public interface OnSeekBarChangeListener { void onProgressChanged(SeekBar s, int p, boolean fromUser); void onStartTrackingTouch(SeekBar s); void onStopTrackingTouch(SeekBar s); }
    private OnSeekBarChangeListener listener;
    public SeekBar(Context c) { this(c, null); }
    public SeekBar(Context c, AttributeSet a) { this(c, a, 16842875 /*seekBarStyle*/); }
    public void setOnSeekBarChangeListener(OnSeekBarChangeListener l) { listener = l; }
    public void setProgress(int p) { super.setProgress(p); if (listener != null) listener.onProgressChanged(this, getProgress(), false); }
    /** indroid: user dragged the host slider. */
    public void indroidUserProgress(int p) {
        if (listener != null) listener.onStartTrackingTouch(this);
        super.setProgress(p);
        if (listener != null) { listener.onProgressChanged(this, getProgress(), true); listener.onStopTrackingTouch(this); }
    }
    protected String indroidType() { return "SeekBar"; }
    public SeekBar(Context c,AttributeSet a,int s) { super(c,a,s); setIndeterminate(false); } public SeekBar(Context c,AttributeSet a,int s,int r) { super(c,a,s,r); setIndeterminate(false); }
}
