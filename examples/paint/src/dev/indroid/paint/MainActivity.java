package dev.indroid.paint;

import android.app.Activity;
import android.os.Bundle;
import android.view.View;
import android.view.MotionEvent;
import android.widget.Button;
import android.widget.LinearLayout;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;

public class MainActivity extends Activity {
    public void onCreate(Bundle state) {
        super.onCreate(state);
        LinearLayout root = new LinearLayout(this); root.setOrientation(LinearLayout.VERTICAL);
        LinearLayout row = new LinearLayout(this); row.setOrientation(LinearLayout.HORIZONTAL);
        final Board board = new Board();
        Button clear = new Button(this); clear.setText("Clear"); clear.setOnClickListener(new View.OnClickListener() { public void onClick(View v) { board.clear(); } }); row.addView(clear);
        String[] labels = {"Coral", "Blue", "Gold"}; final int[] colors = {0xffe95958,0xff3579c9,0xffe6a02e};
        for (int i=0;i<labels.length;i++) { final int color=colors[i]; Button b=new Button(this); b.setText(labels[i]); b.setOnClickListener(new View.OnClickListener() { public void onClick(View v) { board.color=color; } }); row.addView(b); }
        root.addView(row); root.addView(board,new LinearLayout.LayoutParams(-1,0,1)); setContentView(root);
    }
    class Board extends View {
        private final Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Path path=new Path(); int color=0xffe95958; float x=55,y=70,dx=3,dy=2;
        Board() { super(MainActivity.this); }
        void clear() { path.reset(); invalidate(); }
        protected void onDraw(Canvas c) {
            c.drawColor(0xfffffaf1);
            paint.setStyle(Paint.Style.FILL); paint.setColor(0xffe8f0fa); c.drawRect(12,12,getWidth()-12,getHeight()-12,paint);
            paint.setColor(0xffffffff); c.drawRoundRect(20,20,180,58,12,12,paint);
            paint.setColor(0xff34495e); paint.setTextSize(16); c.drawText("Draw & play",32,45,paint);
            paint.setStrokeWidth(2); c.drawLine(20,65,getWidth()-20,65,paint);
            paint.setColor(0xffe6a02e); c.drawCircle(x,y,12,paint);
            paint.setColor(0xff3579c9); c.drawCircle(getWidth()-x,getHeight()-y,9,paint);
            paint.setColor(color); paint.setStyle(Paint.Style.STROKE); paint.setStrokeWidth(5); paint.setStrokeCap(Paint.Cap.ROUND); c.drawPath(path,paint);
            x+=dx; y+=dy; if(x<12 || x>getWidth()-12) dx=-dx; if(y<75 || y>getHeight()-12) dy=-dy;
            invalidate();
        }
        public boolean onTouchEvent(MotionEvent e) { switch(e.getActionMasked()) { case MotionEvent.ACTION_DOWN: path.moveTo(e.getX(),e.getY()); break; case MotionEvent.ACTION_MOVE: case MotionEvent.ACTION_UP: path.lineTo(e.getX(),e.getY()); break; default: return true; } invalidate(); return true; }
    }
}
