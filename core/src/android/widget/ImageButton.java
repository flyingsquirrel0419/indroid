package android.widget;
import android.content.Context;
import android.util.AttributeSet;
public class ImageButton extends ImageView {
    public ImageButton(Context c) { super(c); setClickable(true); }
    public ImageButton(Context c, AttributeSet a) { this(c, a, 16842866 /*imageButtonStyle*/); }
    public ImageButton(Context c, AttributeSet a, int s) { super(c, a, s); setClickable(true); }
    protected String indroidType() { return "ImageButton"; }
    public ImageButton(Context c, AttributeSet a, int s, int r) { this(c,a,s); }
}
