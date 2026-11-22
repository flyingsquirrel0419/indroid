package android.widget;

import android.content.Context;
import android.graphics.Typeface;
import android.text.Editable;
import android.text.SpannableStringBuilder;
import android.text.TextWatcher;
import android.util.AttributeSet;
import android.view.Gravity;
import android.view.Json;
import android.view.View;
import android.view.XmlAttrs;
import java.util.ArrayList;

public class TextView extends View {
    private android.text.method.KeyListener keyListener;
    private android.view.ActionMode.Callback selectionCallback;
    private android.view.textclassifier.TextClassifier textClassifier = android.view.textclassifier.TextClassifier.NO_OP;
    public android.text.method.KeyListener getKeyListener() { return keyListener; }
    public void setKeyListener(android.text.method.KeyListener listener) { keyListener = listener; }
    public android.view.ActionMode.Callback getCustomSelectionActionModeCallback() { return selectionCallback; }
    public void setCustomSelectionActionModeCallback(android.view.ActionMode.Callback callback) { selectionCallback = callback; }
    public android.view.textclassifier.TextClassifier getTextClassifier() { return textClassifier; }
    public void setTextClassifier(android.view.textclassifier.TextClassifier classifier) { textClassifier = classifier == null ? android.view.textclassifier.TextClassifier.NO_OP : classifier; }
    public android.text.PrecomputedText.Params getTextMetricsParams() { return new android.text.PrecomputedText.Params.Builder(getPaint()).build(); }

    public enum BufferType { NORMAL, SPANNABLE, EDITABLE }
    public interface OnEditorActionListener { boolean onEditorAction(TextView v, int actionId, android.view.KeyEvent event); }
    CharSequence text = "";
    private CharSequence hint;
    private float textSize = 14f;
    private int textColor;
    private boolean hasTextColor;
    private int hintColor;
    private int gravity;
    private int style;
    private boolean allCaps;
    private int maxLines = -1;
    private int inputType;
    private CharSequence error;
    ArrayList<TextWatcher> watchers;
    private OnEditorActionListener editorListener;

    public TextView(Context c) { this(c, null); }
    public TextView(Context c, AttributeSet a) { this(c, a, 16842884 /*textViewStyle*/); }
    public TextView(Context c, AttributeSet a, int s) { this(c, a, s, 0); }
    public TextView(Context c, AttributeSet a, int s, int r) {
        super(c, a, s, r);
        if (c == null) return;
        android.content.res.TypedArray t = c.obtainStyledAttributes(a, TEXT_ATTRS, s, r);
        int ap = t.getResourceId(0, 0);
        if (ap != 0) applyAppearance(c.obtainStyledAttributes(ap, APPEARANCE_ATTRS));
        if (t.hasValue(1)) setTextColor(t.getColorStateList(1));
        if (t.hasValue(2)) textSize = t.getDimension(2, textSize);
        if (t.hasValue(3)) style = t.getInt(3, style);
        if (t.hasValue(4)) { android.content.res.ColorStateList h = t.getColorStateList(4); if (h != null) hintColor = h.getDefaultColor(); }
        CharSequence tx = t.getText(5);
        if (tx != null) text = tx;
        CharSequence hn = t.getText(6);
        if (hn != null) hint = hn;
        gravity = t.getInt(7, gravity);
        if (t.getBoolean(10, false)) maxLines = 1;
        maxLines = t.getInt(8, t.getInt(9, maxLines));
        inputType = t.getInt(11, inputType);
        if (t.hasValue(12)) allCaps = t.getBoolean(12, false);
        if (t.hasValue(13)) fontFamily = t.getString(13);
        t.recycle();
    }
    static final int[] TEXT_ATTRS = { 16842804 /*textAppearance*/, 16842904 /*textColor*/, 16842901 /*textSize*/, 16842903 /*textStyle*/,
        16842906 /*textColorHint*/, 16843087 /*text*/, 16843088 /*hint*/, 16842927 /*gravity*/, 16843091 /*maxLines*/, 16843092 /*lines*/,
        16843101 /*singleLine*/, 16843296 /*inputType*/, 16843660 /*textAllCaps*/, 16843692 /*fontFamily*/ };
    static final int[] APPEARANCE_ATTRS = { 16842904 /*textColor*/, 16842901 /*textSize*/, 16842903 /*textStyle*/, 16842906 /*textColorHint*/,
        16843660 /*textAllCaps*/, 16843692 /*fontFamily*/ };
    private String fontFamily;
    private android.content.res.ColorStateList textColors;
    private void applyAppearance(android.content.res.TypedArray t) {
        if (t.hasValue(0)) { android.content.res.ColorStateList c = t.getColorStateList(0); if (c != null) setTextColor(c); }
        if (t.hasValue(1)) textSize = t.getDimension(1, textSize);
        if (t.hasValue(2)) style = t.getInt(2, style);
        if (t.hasValue(3)) { android.content.res.ColorStateList h = t.getColorStateList(3); if (h != null) hintColor = h.getDefaultColor(); }
        if (t.hasValue(4)) allCaps = t.getBoolean(4, false);
        if (t.hasValue(5)) fontFamily = t.getString(5);
        t.recycle();
    }
    public void setTextAppearance(int resId) { setTextAppearance(getContext(), resId); }
    public void setTextAppearance(Context c, int resId) { if (resId != 0 && c != null) { applyAppearance(c.obtainStyledAttributes(resId, APPEARANCE_ATTRS)); requestLayout(); } }
    protected void drawableStateChanged() { super.drawableStateChanged(); updateTextColor(); }
    private void updateTextColor() {
        if (textColors == null) return;
        int c = textColors.getColorForState(getDrawableState(), textColors.getDefaultColor());
        if (c != textColor) { textColor = c; invalidate(); }
    }
    public void setText(CharSequence t) { setText(t, BufferType.NORMAL); }
    public void setText(CharSequence t, BufferType type) {
        CharSequence old = text;
        if (t == null) t = "";
        android.text.Spanned dest = new SpannableStringBuilder(old);
        for (android.text.InputFilter filter : filters) { CharSequence changed=filter.filter(t,0,t.length(),dest,0,old.length()); if(changed!=null) t=changed; }
        requestLayout();
        if (watchers != null) for (TextWatcher w : new ArrayList<TextWatcher>(watchers)) w.beforeTextChanged(old, 0, old.length(), t == null ? 0 : t.length());
        text = t == null ? "" : t;
        onSetText();
        onTextChanged(text, 0, old.length(), text.length());
        if (watchers != null) {
            for (TextWatcher w : new ArrayList<TextWatcher>(watchers)) w.onTextChanged(text, 0, old.length(), text.length());
            Editable e = this instanceof EditText ? ((EditText) this).getText() : new SpannableStringBuilder(text);
            for (TextWatcher w : new ArrayList<TextWatcher>(watchers)) w.afterTextChanged(e);
        }
    }
    void onSetText() {}
    public final void setText(int resid) { setText(getContext().getResources().getText(resid)); }
    public final void setText(char[] t, int start, int len) { setText(new String(t, start, len)); }
    public CharSequence getText() { return text; }
    public void append(CharSequence t) { setText(text.toString() + t); }
    public int length() { return text.length(); }
    public void setHint(CharSequence h) { hint = h; requestLayout(); }
    public final void setHint(int resid) { hint = getContext().getResources().getText(resid); }
    public CharSequence getHint() { return hint; }
    public void setTextSize(float sp) { textSize = sp; requestLayout(); }
    public void setTextSize(int unit, float size) { textSize = unit == 0 ? size : size; requestLayout(); }
    public float getTextSize() { return textSize; }
    public void setTextColor(int c) { textColors = android.content.res.ColorStateList.valueOf(c); textColor = c; hasTextColor = true; }
    public void setTextColor(android.content.res.ColorStateList c) { if (c == null) return; textColors = c; hasTextColor = true; textColor = c.getDefaultColor(); updateTextColor(); }
    public final int getCurrentTextColor() { return hasTextColor ? textColor : 0xde000000; }
    public void setHintTextColor(int c) { hintColor = c; }
    public void setGravity(int g) { gravity = g; }
    public int getGravity() { return gravity; }
    public void setTypeface(Typeface tf) { style = tf == null ? 0 : tf.getStyle(); requestLayout(); }
    public void setTypeface(Typeface tf, int s) { style = s; requestLayout(); }
    public Typeface getTypeface() { return Typeface.defaultFromStyle(style); }
    public void setAllCaps(boolean b) { allCaps = b; requestLayout(); }
    public void setMaxLines(int n) { maxLines = n; requestLayout(); }
    public void setLines(int n) { maxLines = n; requestLayout(); }
    public void setMinLines(int n) { minLines=n; requestLayout(); }
    public void setSingleLine() { maxLines = 1; requestLayout(); }
    public void setSingleLine(boolean b) { maxLines = b ? 1 : -1; requestLayout(); }
    public void setEllipsize(android.text.TextUtils.TruncateAt w) {}
    public void setLineSpacing(float add, float mult) { spacingExtra=add; spacingMult=mult; requestLayout(); }
    public void setLetterSpacing(float s) { letterSpacing=s; requestLayout(); }
    public void setIncludeFontPadding(boolean b) { includeFontPadding=b; requestLayout(); }
    public void setMovementMethod(android.text.method.MovementMethod m) {}
    public void setTransformationMethod(android.text.method.TransformationMethod m) {
        transformation=m;
        if (m instanceof android.text.method.PasswordTransformationMethod) inputType = 0x81; else if (inputType == 0x81) inputType = 1;
    }
    public void setInputType(int t) { inputType = t; }
    public int getInputType() { return inputType; }
    public void setImeOptions(int o) { imeOptions=o; }
    public void setOnEditorActionListener(OnEditorActionListener l) { editorListener = l; }
    public void setError(CharSequence e) { error = e; }
    public CharSequence getError() { return error; }
    public void setCompoundDrawablesWithIntrinsicBounds(int l, int t, int r, int b) { setCompoundDrawables(drawable(l),drawable(t),drawable(r),drawable(b)); }
    public void setCompoundDrawables(android.graphics.drawable.Drawable l, android.graphics.drawable.Drawable t, android.graphics.drawable.Drawable r, android.graphics.drawable.Drawable b) { compound=new android.graphics.drawable.Drawable[]{l,t,r,b}; requestLayout(); }
    public void setCompoundDrawablePadding(int p) { compoundPadding=p; requestLayout(); }
    public void setHorizontallyScrolling(boolean b) { horizontallyScrolling=b; }
    public void setTextIsSelectable(boolean b) {}
    public void setCursorVisible(boolean b) {}
    public void setSelection(int i) { selectionStart=selectionEnd=i; onSelectionChanged(i,i); }
    public void addTextChangedListener(TextWatcher w) { if (watchers == null) watchers = new ArrayList<TextWatcher>(); watchers.add(w); }
    public void removeTextChangedListener(TextWatcher w) { if (watchers != null) watchers.remove(w); }
    /** indroid: user typed in the host field. */
    public void indroidUserText(String t) { setText(t); }
    /** indroid: return key pressed. */
    public void indroidEditorAction() { if (editorListener != null) editorListener.onEditorAction(this, 6, null); }

    /** Text as shown (all-caps and password transforms applied). */
    CharSequence displayText() {
        CharSequence t = text.length() > 0 ? text : (hint != null ? hint : "");
        if (allCaps) t = t.toString().toUpperCase();
        if ((inputType & 0xfff) == 0x81 || (inputType & 0xfff) == 0x12 || (inputType & 0xfff) == 0xe1) {
            StringBuilder b = new StringBuilder(); for (int i = 0; i < t.length(); i++) b.append('\u2022'); t = b;
        }
        return t;
    }
    int textFlags() {
        int f = (style & 1) != 0 ? android.view.VMText.BOLD : 0;
        if ((style & 2) != 0) f |= android.view.VMText.ITALIC;
        if (maxLines > 0) f |= Math.min(maxLines, 255) << 8;
        return f;
    }
    protected void onMeasure(int ws, int hs) {
        int wMode = MeasureSpec.getMode(ws), wSize = MeasureSpec.getSize(ws);
        int hMode = MeasureSpec.getMode(hs), hSize = MeasureSpec.getSize(hs);
        String shown = displayText().toString();
        int padH = getPaddingLeft() + getPaddingRight(), padV = getPaddingTop() + getPaddingBottom();
        int extraW = indroidExtraWidth();
        int width;
        if (wMode == MeasureSpec.EXACTLY) width = wSize;
        else {
            float max = wMode == MeasureSpec.AT_MOST ? Math.max(0, wSize - padH - extraW) : 0;
            long m = android.view.VMText.measure(shown, textSize, textFlags(), max);
            width = android.view.VMText.widthOf(m) + padH + extraW;
            width = Math.max(width, getSuggestedMinimumWidth());
            if (wMode == MeasureSpec.AT_MOST) width = Math.min(width, wSize);
        }
        long m = android.view.VMText.measure(shown, textSize, textFlags(), Math.max(1, width - padH - extraW));
        int height;
        if (hMode == MeasureSpec.EXACTLY) height = hSize;
        else {
            height = Math.max(android.view.VMText.heightOf(m), indroidExtraHeight()) + padV;
            height = Math.max(height, getSuggestedMinimumHeight());
            if (hMode == MeasureSpec.AT_MOST) height = Math.min(height, hSize);
        }
        setMeasuredDimension(width, height);
    }
    /** indroid: room for a compound button / switch beside the text. */
    int indroidExtraWidth() { return 0; }
    int indroidExtraHeight() { return 0; }
    public int getLineHeight() { return lineHeight>0 ? lineHeight : Math.round(textSize * 1.17f); }
    public int getLineCount() { return Math.max(1, android.view.VMText.heightOf(android.view.VMText.measure(displayText().toString(), textSize, textFlags(), Math.max(1, getWidth() - getPaddingLeft() - getPaddingRight()))) / Math.max(1, getLineHeight())); }
    public int getBaseline() { return getPaddingTop() + Math.round(textSize * 0.93f); }
    public int getMaxLines() { return maxLines; }
    public boolean isAllCaps() { return allCaps; }
    protected String indroidType() { return "TextView"; }
    protected void indroidProps(StringBuilder sb) {
        Json.key(sb, "text"); Json.str(sb, allCaps ? text.toString().toUpperCase() : text);
        if (hint != null) { Json.key(sb, "hint"); Json.str(sb, hint); }
        Json.key(sb, "textSize"); Json.num(sb, textSize);
        if (hasTextColor) { Json.key(sb, "color"); Json.str(sb, android.graphics.Color.toHex(textColor)); }
        if (hintColor != 0) { Json.key(sb, "hintColor"); Json.str(sb, android.graphics.Color.toHex(hintColor)); }
        String g = Gravity.describe(gravity);
        if (g != null) { Json.key(sb, "gravity"); Json.str(sb, g); }
        if ((style & 1) != 0) { Json.key(sb, "bold"); sb.append("true"); }
        if ((style & 2) != 0) { Json.key(sb, "italic"); sb.append("true"); }
        if (maxLines > 0) { Json.key(sb, "maxLines"); sb.append(maxLines); }
        if (inputType != 0) { Json.key(sb, "inputType"); sb.append(inputType); }
        if (error != null) { Json.key(sb, "errorText"); Json.str(sb, error); }
    }

    private android.graphics.drawable.Drawable[] compound=new android.graphics.drawable.Drawable[4];
    private android.content.res.ColorStateList compoundTint;
    private android.graphics.PorterDuff.Mode compoundTintMode;
    private int compoundPadding, minLines=1, breakStrategy, hyphenation, imeOptions, lineHeight, selectionStart=-1, selectionEnd=-1;
    private float spacingExtra, spacingMult=1, letterSpacing;
    private boolean includeFontPadding=true, horizontallyScrolling;
    private java.util.Locale textLocale=java.util.Locale.getDefault();
    private android.graphics.drawable.Drawable drawable(int id) { return id==0?null:getResources().getDrawable(id); }
    public android.graphics.drawable.Drawable[] getCompoundDrawables() { return compound.clone(); }
    public android.graphics.drawable.Drawable[] getCompoundDrawablesRelative() { return getCompoundDrawables(); }
    public void setCompoundDrawablesWithIntrinsicBounds(android.graphics.drawable.Drawable l,android.graphics.drawable.Drawable t,android.graphics.drawable.Drawable r,android.graphics.drawable.Drawable b) { setCompoundDrawables(l,t,r,b); }
    public void setCompoundDrawablesRelative(android.graphics.drawable.Drawable l,android.graphics.drawable.Drawable t,android.graphics.drawable.Drawable r,android.graphics.drawable.Drawable b) { setCompoundDrawables(l,t,r,b); }
    public void setCompoundDrawablesRelativeWithIntrinsicBounds(android.graphics.drawable.Drawable l,android.graphics.drawable.Drawable t,android.graphics.drawable.Drawable r,android.graphics.drawable.Drawable b) { setCompoundDrawables(l,t,r,b); }
    public void setCompoundDrawablesRelativeWithIntrinsicBounds(int l,int t,int r,int b) { setCompoundDrawablesWithIntrinsicBounds(l,t,r,b); }
    public void setCompoundDrawableTintList(android.content.res.ColorStateList c) { compoundTint=c; } public android.content.res.ColorStateList getCompoundDrawableTintList() { return compoundTint; }
    public void setCompoundDrawableTintMode(android.graphics.PorterDuff.Mode m) { compoundTintMode=m; } public android.graphics.PorterDuff.Mode getCompoundDrawableTintMode() { return compoundTintMode; }
    public int getCompoundDrawablePadding() { return compoundPadding; } public int getCompoundPaddingLeft() { return getPaddingLeft(); } public int getCompoundPaddingRight() { return getPaddingRight(); } public int getCompoundPaddingTop() { return getPaddingTop(); } public int getCompoundPaddingBottom() { return getPaddingBottom(); } public int getTotalPaddingLeft() { return getCompoundPaddingLeft(); } public int getTotalPaddingRight() { return getCompoundPaddingRight(); }

    public void setLinkTextColor(int color) {} public void setLinkTextColor(android.content.res.ColorStateList color) {}
    public void setShadowLayer(float radius,float dx,float dy,int color) {} public void setHighlightColor(int color) {}
    public void setAutoSizeTextTypeWithDefaults(int type) {} public void setAutoSizeTextTypeUniformWithConfiguration(int min,int max,int step,int unit) {} public void setAutoSizeTextTypeUniformWithPresetSizes(int[] sizes,int unit) {} public int getAutoSizeTextType() { return 0; } public int getAutoSizeMinTextSize() { return -1; } public int getAutoSizeMaxTextSize() { return -1; } public int getAutoSizeStepGranularity() { return -1; } public int[] getAutoSizeTextAvailableSizes() { return new int[0]; }
    public void setBreakStrategy(int v) { breakStrategy=v; } public int getBreakStrategy() { return breakStrategy; } public void setHyphenationFrequency(int v) { hyphenation=v; } public int getHyphenationFrequency() { return hyphenation; }
    public void setRawInputType(int v) { inputType=v; } public int getImeOptions() { return imeOptions; } public void setImeActionLabel(CharSequence label,int id) {}
    public void setTextLocale(java.util.Locale l) { textLocale=l; } public java.util.Locale getTextLocale() { return textLocale; } public void setFontFeatureSettings(String s) {} public boolean setFontVariationSettings(String s) { return false; }
    public void setLineHeight(int h) { lineHeight=h; requestLayout(); } public void setLineHeight(int unit,float h) { setLineHeight((int)h); }
    public void setFirstBaselineToTopHeight(int h) { if (h>getBaseline()) setPadding(getPaddingLeft(),getPaddingTop()+h-getBaseline(),getPaddingRight(),getPaddingBottom()); }
    public void setLastBaselineToBottomHeight(int h) { if(h>0) setPadding(getPaddingLeft(),getPaddingTop(),getPaddingRight(),h); }
    public int getMinLines() { return minLines; } public float getLineSpacingExtra() { return spacingExtra; } public float getLineSpacingMultiplier() { return spacingMult; } public float getLetterSpacing() { return letterSpacing; } public boolean getIncludeFontPadding() { return includeFontPadding; } public boolean isHorizontallyScrollable() { return horizontallyScrolling; }
    public void setMinWidth(int v) { setMinimumWidth(v); } public void setMinHeight(int v) { setMinimumHeight(v); } public int getMinWidth() { return getMinimumWidth(); } public int getMinHeight() { return getMinimumHeight(); } public void setMaxWidth(int v) {} public void setMinEms(int v) {} public void setMaxEms(int v) {}
    public android.content.res.ColorStateList getTextColors() { return textColors != null ? textColors : android.content.res.ColorStateList.valueOf(getCurrentTextColor()); } public android.content.res.ColorStateList getHintTextColors() { return android.content.res.ColorStateList.valueOf(hintColor); }
    public void beginBatchEdit() {} public void endBatchEdit() {} protected void onTextChanged(CharSequence text,int start,int before,int count) {} protected void onSelectionChanged(int start,int end) {} public int getSelectionStart() { return selectionStart; } public int getSelectionEnd() { return selectionEnd; }
    public int getLineBounds(int line,android.graphics.Rect bounds) { int top=getPaddingTop()+line*getLineHeight(); if(bounds!=null) bounds.set(getPaddingLeft(),top,getWidth()-getPaddingRight(),top+getLineHeight()); return top+getBaseline(); } public int getOffsetForPosition(float x,float y) { return Math.max(0,Math.min(text.length(),(int)(x/Math.max(1,textSize*.55f)))); }

 private android.text.method.TransformationMethod transformation; public android.text.method.TransformationMethod getTransformationMethod() { return transformation; } public android.text.Editable getEditableText() { return this instanceof EditText ? ((EditText)this).getText() : null; } public boolean onCheckIsTextEditor() { return this instanceof EditText; } public void setImeHintLocales(android.os.LocaleList l) {} public void setTextLocales(android.os.LocaleList l) { if(l!=null && !l.isEmpty()) setTextLocale(l.get(0)); }
    private final android.text.TextPaint paint=new android.text.TextPaint();
    private android.text.InputFilter[] filters=new android.text.InputFilter[0];
    public void setFilters(android.text.InputFilter[] f) { if(f==null) throw new IllegalArgumentException("filters"); filters=f.clone(); }
    public android.text.InputFilter[] getFilters() { return filters.clone(); }
    public android.text.TextPaint getPaint() { paint.setTextSize(textSize); paint.setColor(getCurrentTextColor()); paint.setTypeface(getTypeface()); paint.setLetterSpacing(letterSpacing); return paint; }
    public android.text.Layout getLayout() { if(getWidth()<=0) return null; return new android.text.StaticLayout(text,getPaint(),Math.max(0,getWidth()-getCompoundPaddingLeft()-getCompoundPaddingRight()),android.text.Layout.Alignment.ALIGN_NORMAL,spacingMult,spacingExtra,includeFontPadding); }
    public android.text.TextDirectionHeuristic getTextDirectionHeuristic() { return android.text.TextDirectionHeuristics.FIRSTSTRONG_LTR; }
    public void setEditableFactory(android.text.Editable.Factory factory) {}
}
