package android.app;

import android.content.Context;
import android.content.DialogInterface;
import android.view.Json;
import android.view.View;

public class AlertDialog extends Dialog {
    CharSequence message;
    final CharSequence[] buttonText = new CharSequence[3];
    final DialogInterface.OnClickListener[] buttonListener = new DialogInterface.OnClickListener[3];
    CharSequence[] items;
    DialogInterface.OnClickListener itemsListener;

    protected AlertDialog(Context c) { super(c); }
    public void setMessage(CharSequence m) { message = m; }
    public void setButton(int which, CharSequence text, DialogInterface.OnClickListener l) { buttonText[-which - 1] = text; buttonListener[-which - 1] = l; }
    public android.widget.Button getButton(int which) { return null; }
    public void setView(View v) { content = v; }

    void indroidButton(int which) {
        if (which >= 0) { if (itemsListener != null) itemsListener.onClick(this, which); dismiss(); return; }
        DialogInterface.OnClickListener l = buttonListener[-which - 1];
        if (l != null) l.onClick(this, which);
        dismiss();
    }
    void indroidJson(StringBuilder sb) {
        sb.append("{\"title\":"); Json.str(sb, title);
        sb.append(",\"message\":"); Json.str(sb, message);
        if (content != null) { sb.append(",\"content\":"); content.indroidJson(sb); }
        sb.append(",\"buttons\":[");
        boolean first = true;
        for (int i = 0; i < 3; i++) {
            if (buttonText[i] == null) continue;
            if (!first) sb.append(',');
            first = false;
            sb.append("{\"which\":").append(-i - 1).append(",\"text\":"); Json.str(sb, buttonText[i]); sb.append('}');
        }
        sb.append(']');
        if (items != null) {
            sb.append(",\"items\":[");
            for (int i = 0; i < items.length; i++) { if (i > 0) sb.append(','); Json.str(sb, items[i]); }
            sb.append(']');
        }
        sb.append(",\"cancelable\":").append(cancelable).append('}');
    }

    public static class Builder {
        private final Context ctx;
        private final AlertDialog d;
        public Builder(Context c) { ctx = c; d = new AlertDialog(c); }
        public Builder(Context c, int theme) { this(c); }
        public Context getContext() { return ctx; }
        public Builder setTitle(CharSequence t) { d.title = t; return this; }
        public Builder setTitle(int r) { d.title = ctx.getText(r); return this; }
        public Builder setMessage(CharSequence m) { d.message = m; return this; }
        public Builder setMessage(int r) { d.message = ctx.getText(r); return this; }
        public Builder setIcon(int r) { return this; }
        public Builder setView(View v) { d.content = v; return this; }
        public Builder setView(int layout) { d.content = android.view.LayoutInflater.from(ctx).inflate(layout, null); return this; }
        public Builder setCancelable(boolean c) { d.cancelable = c; return this; }
        public Builder setPositiveButton(CharSequence t, DialogInterface.OnClickListener l) { d.setButton(BUTTON_POSITIVE, t, l); return this; }
        public Builder setPositiveButton(int r, DialogInterface.OnClickListener l) { return setPositiveButton(ctx.getText(r), l); }
        public Builder setNegativeButton(CharSequence t, DialogInterface.OnClickListener l) { d.setButton(BUTTON_NEGATIVE, t, l); return this; }
        public Builder setNegativeButton(int r, DialogInterface.OnClickListener l) { return setNegativeButton(ctx.getText(r), l); }
        public Builder setNeutralButton(CharSequence t, DialogInterface.OnClickListener l) { d.setButton(BUTTON_NEUTRAL, t, l); return this; }
        public Builder setNeutralButton(int r, DialogInterface.OnClickListener l) { return setNeutralButton(ctx.getText(r), l); }
        public Builder setItems(CharSequence[] items, DialogInterface.OnClickListener l) { d.items = items; d.itemsListener = l; return this; }
        public Builder setOnCancelListener(DialogInterface.OnCancelListener l) { d.onCancel = l; return this; }
        public Builder setOnDismissListener(DialogInterface.OnDismissListener l) { d.onDismiss = l; return this; }
        public AlertDialog create() { return d; }
        public AlertDialog show() { d.show(); return d; }
        public Builder setCustomTitle(View v) { return this; }
        public Builder setIcon(android.graphics.drawable.Drawable icon) { return this; }
        public Builder setOnKeyListener(DialogInterface.OnKeyListener l) { d.setOnKeyListener(l); return this; }
        public Builder setSingleChoiceItems(CharSequence[] items,int checked,DialogInterface.OnClickListener l) { return setItems(items,l); }
        public Builder setMultiChoiceItems(CharSequence[] items,boolean[] checked,DialogInterface.OnMultiChoiceClickListener listener) { d.items=items; return this; }
        public Builder setAdapter(android.widget.ListAdapter adapter,DialogInterface.OnClickListener listener) { return this; }
        public Builder setView(View view,int left,int top,int right,int bottom) { return setView(view); }
    }
}
