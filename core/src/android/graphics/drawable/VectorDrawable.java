package android.graphics.drawable;

import android.content.res.ColorStateList;
import android.content.res.Resources;
import android.content.res.VMValue;
import android.content.res.VMXml;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.PathParser;
import android.graphics.PorterDuff;
import android.graphics.Rect;
import java.util.ArrayList;

public class VectorDrawable extends Drawable {
 private int width=-1,height=-1; private float viewportW,viewportH; private Group root=new Group();
 private ColorStateList tint; private PorterDuff.Mode tintMode=PorterDuff.Mode.SRC_IN;
 private static class Group {
  String name; float rotation,pivotX,pivotY,translateX,translateY,scaleX=1,scaleY=1; ArrayList<Object> children=new ArrayList<Object>();
  Group() {} Group(Group other) { name=other.name;rotation=other.rotation;pivotX=other.pivotX;pivotY=other.pivotY;translateX=other.translateX;translateY=other.translateY;scaleX=other.scaleX;scaleY=other.scaleY;for(Object o:other.children)children.add(o instanceof Group?new Group((Group)o):new VPath((VPath)o)); }
 }
 private static class VPath {
  String name; Path path; int fill,stroke; float fillAlpha=1,strokeAlpha=1,strokeWidth; Paint.Cap cap=Paint.Cap.BUTT; Paint.Join join=Paint.Join.MITER; boolean clip;
  VPath() {} VPath(VPath v){name=v.name;path=new Path(v.path);fill=v.fill;stroke=v.stroke;fillAlpha=v.fillAlpha;strokeAlpha=v.strokeAlpha;strokeWidth=v.strokeWidth;cap=v.cap;join=v.join;clip=v.clip;}
 }
 public VectorDrawable() {}
 private VectorDrawable(VectorDrawable v) { width=v.width;height=v.height;viewportW=v.viewportW;viewportH=v.viewportH;root=new Group(v.root);tint=v.tint;tintMode=v.tintMode;setAlpha(v.getAlpha());setAutoMirrored(v.isAutoMirrored()); }
 public void setIntrinsicSize(int w,int h){width=w;height=h;} public int getIntrinsicWidth(){return width;} public int getIntrinsicHeight(){return height;}
 private static String attr(VMXml n,int id,String name,Resources res,Resources.Theme theme) {
  String v=n.get(id);if(v==null)v=n.get(name);
  for(int i=0;i<16 && v!=null;i++) { char k=VMValue.kind(v); if(k=='a' && theme!=null) v=theme.indroidAttr(VMValue.intOf(v,0)); else if(k=='r' && res!=null) { String next=res.indroidValue(VMValue.intOf(v,0));if(next==null)break;v=next; } else break; }
  return v;
 }
 private static float num(VMXml n,int id,String name,Resources r,Resources.Theme t,float def){return VMValue.floatOf(attr(n,id,name,r,t),def);}
 private static int color(VMXml n,int id,String name,Resources r,Resources.Theme t,int def) {
  String original=n.get(id);if(original==null)original=n.get(name);String v=attr(n,id,name,r,t); if(v==null)return def;
  if(VMValue.kind(v)=='p' && r!=null){ColorStateList list=ColorStateList.indroidInflate(r,VMValue.body(v),t);return list==null?def:list.getDefaultColor();}
  if(VMValue.kind(v)=='r' && r!=null) {try{return r.getColor(VMValue.intOf(v,0),t);}catch(RuntimeException ignored){return def;}}
  if(VMValue.kind(v)=='s' && VMValue.body(v).startsWith("#")){try{return android.graphics.Color.parseColor(VMValue.body(v));}catch(RuntimeException ignored){return def;}}
  return VMValue.intOf(v,def);
 }
 private static int choice(String v,String a,String b,int def) { if(v==null)return def;String s=VMValue.body(v);if(s.equals(a))return 1;if(s.equals(b))return 2;return VMValue.intOf(v,def); }
 public static VectorDrawable indroidInflate(VMXml n,Resources r,Resources.Theme t) {
  VectorDrawable v=new VectorDrawable(); if(n==null)return v;
  v.width=(int)num(n,0x01010159,"width",r,t,-1);v.height=(int)num(n,0x01010155,"height",r,t,-1);
  v.viewportW=num(n,0x01010402,"viewportWidth",r,t,0);v.viewportH=num(n,0x01010403,"viewportHeight",r,t,0);
  v.setAlpha(Math.round(Math.max(0,Math.min(1,num(n,0x0101031f,"alpha",r,t,1)))*255));
  v.setAutoMirrored(VMValue.boolOf(attr(n,0x010103ea,"autoMirrored",r,t),false));
  if(n.get(0x01010121)!=null || n.get("tint")!=null)v.setTint(color(n,0x01010121,"tint",r,t,0));
  String mode=attr(n,0x010103fb,"tintMode",r,t); if(mode!=null) {int m=VMValue.intOf(mode,5);String s=VMValue.body(mode);if(m==3||s.equals("src_over"))v.tintMode=PorterDuff.Mode.SRC_OVER;else if(m==9||s.equals("src_atop"))v.tintMode=PorterDuff.Mode.SRC_ATOP;else if(m==14||s.equals("multiply"))v.tintMode=PorterDuff.Mode.MULTIPLY;else if(m==15||s.equals("screen"))v.tintMode=PorterDuff.Mode.SCREEN;else if(m==16||s.equals("add"))v.tintMode=PorterDuff.Mode.ADD;}
  inflateChildren(v.root,n,r,t);return v;
 }
 private static void inflateChildren(Group g,VMXml n,Resources r,Resources.Theme t) {
  for(VMXml c:n.children) {
   if(c.tag.equals("group")) { Group x=new Group();x.name=VMValue.body(attr(c,0x01010003,"name",r,t));x.rotation=num(c,0x01010326,"rotation",r,t,0);x.pivotX=num(c,0x010101b5,"pivotX",r,t,0);x.pivotY=num(c,0x010101b6,"pivotY",r,t,0);x.scaleX=num(c,0x01010324,"scaleX",r,t,1);x.scaleY=num(c,0x01010325,"scaleY",r,t,1);x.translateX=num(c,0x0101045a,"translateX",r,t,0);x.translateY=num(c,0x0101045b,"translateY",r,t,0);inflateChildren(x,c,r,t);g.children.add(x);
   } else if(c.tag.equals("path")||c.tag.equals("clip-path")) {VPath p=new VPath();p.name=VMValue.body(attr(c,0x01010003,"name",r,t));p.path=PathParser.createPathFromPathData(VMValue.body(attr(c,0x01010405,"pathData",r,t)));p.clip=c.tag.equals("clip-path");p.fill=color(c,0x01010404,"fillColor",r,t,0);p.stroke=color(c,0x01010406,"strokeColor",r,t,0);p.strokeWidth=num(c,0x01010407,"strokeWidth",r,t,0);p.fillAlpha=num(c,0x010104cc,"fillAlpha",r,t,1);p.strokeAlpha=num(c,0x010104cb,"strokeAlpha",r,t,1);
    int cap=choice(attr(c,0x0101040b,"strokeLineCap",r,t),"round","square",0);p.cap=cap==1?Paint.Cap.ROUND:cap==2?Paint.Cap.SQUARE:Paint.Cap.BUTT;
    int join=choice(attr(c,0x0101040c,"strokeLineJoin",r,t),"round","bevel",0);p.join=join==1?Paint.Join.ROUND:join==2?Paint.Join.BEVEL:Paint.Join.MITER;
    String ft=attr(c,0x0101051e,"fillType",r,t);if(ft!=null && (VMValue.intOf(ft,0)==1 || VMValue.body(ft).equals("evenOdd")))p.path.setFillType(Path.FillType.EVEN_ODD);g.children.add(p);
   }
  }
 }
 public void setTintList(ColorStateList c){tint=c;super.setTintList(c);} public void setTintMode(PorterDuff.Mode m){tintMode=m;super.setTintMode(m);}
 public boolean isStateful(){return tint!=null&&tint.isStateful();} protected boolean onStateChange(int[] state){return isStateful();}
 private int painted(int color,float opacity) {int a=Math.round((color>>>24)*Math.max(0,Math.min(1,opacity))*Math.max(0,Math.min(255,getAlpha()))/255f);if(tint!=null && tintMode==PorterDuff.Mode.SRC_IN)color=(color&0xff000000)|(tint.getColorForState(getState(),tint.getDefaultColor())&0xffffff);return (a<<24)|(color&0xffffff);}
 public void draw(Canvas c){Rect b=getBounds();if(viewportW<=0||viewportH<=0||b.width()<=0||b.height()<=0)return;c.save();c.translate(b.left,b.top);if(isAutoMirrored()&&getLayoutDirection()==1){c.translate(b.width(),0);c.scale(-1,1);}c.scale(b.width()/viewportW,b.height()/viewportH);drawGroup(c,root);c.restore();}
 private void drawGroup(Canvas c,Group g){c.save();c.translate(g.translateX+g.pivotX,g.translateY+g.pivotY);c.rotate(g.rotation);c.scale(g.scaleX,g.scaleY);c.translate(-g.pivotX,-g.pivotY);for(Object o:g.children){if(o instanceof Group){drawGroup(c,(Group)o);continue;}VPath p=(VPath)o;if(p.clip){c.clipPath(p.path);continue;}Paint paint=new Paint();if((p.fill>>>24)!=0){paint.setStyle(Paint.Style.FILL);paint.setColor(painted(p.fill,p.fillAlpha));c.drawPath(p.path,paint);}if((p.stroke>>>24)!=0 && p.strokeWidth>0){paint.setStyle(Paint.Style.STROKE);paint.setColor(painted(p.stroke,p.strokeAlpha));paint.setStrokeWidth(p.strokeWidth);paint.setStrokeCap(p.cap);paint.setStrokeJoin(p.join);c.drawPath(p.path,paint);}}c.restore();}
 public Drawable mutate(){root=new Group(root);return this;}
 public ConstantState getConstantState(){final VectorDrawable copy=new VectorDrawable(this);return new ConstantState(){public Drawable newDrawable(){return new VectorDrawable(copy);}};}
}
