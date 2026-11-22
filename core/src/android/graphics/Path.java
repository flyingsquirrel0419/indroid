package android.graphics;
public class Path {
 public enum Op { DIFFERENCE, INTERSECT, UNION, XOR, REVERSE_DIFFERENCE } public enum FillType { WINDING, EVEN_ODD, INVERSE_WINDING, INVERSE_EVEN_ODD } public enum Direction { CW, CCW }
 private StringBuilder s=new StringBuilder(); private float x,y,startX,startY; private FillType fill=FillType.WINDING;
 public Path() {} public Path(Path p) { set(p); } public void set(Path p) { s=new StringBuilder(p.s.toString()); x=p.x;y=p.y;startX=p.startX;startY=p.startY;fill=p.fill; }
 private void cmd(String c,float... a) { if(s.length()>0) s.append(' '); s.append(c); for(float v:a) { s.append(' '); pathNumber(s,v); } }
 private static void pathNumber(StringBuilder out,float value) {
  String v=Float.toString(value); int e=v.indexOf('E'); if(e<0)e=v.indexOf('e');
  if(e<0) { if(v.endsWith(".0"))v=v.substring(0,v.length()-2); out.append(v); return; }
  int exponent=Integer.parseInt(v.substring(e+1)), start=v.charAt(0)=='-'?1:0;
  if(start==1)out.append('-'); String mant=v.substring(start,e); int dot=mant.indexOf('.');
  int decimal=(dot<0?mant.length():dot)+exponent; String digits=dot<0?mant:mant.substring(0,dot)+mant.substring(dot+1);
  if(decimal<=0){out.append("0.");for(int i=0;i< -decimal;i++)out.append('0');out.append(digits);}
  else if(decimal>=digits.length()){out.append(digits);for(int i=digits.length();i<decimal;i++)out.append('0');}
  else {out.append(digits.substring(0,decimal));out.append('.');out.append(digits.substring(decimal));}
 }
 public void moveTo(float x,float y) { cmd("M",x,y); this.x=startX=x; this.y=startY=y; }
 public void lineTo(float x,float y) { cmd("L",x,y); this.x=x; this.y=y; }
 public void quadTo(float a,float b,float x,float y) { cmd("Q",a,b,x,y); this.x=x; this.y=y; }
 public void cubicTo(float a,float b,float c,float d,float x,float y) { cmd("C",a,b,c,d,x,y); this.x=x; this.y=y; }
 public void close() { cmd("Z"); x=startX; y=startY; }
 public void rMoveTo(float dx,float dy) { moveTo(x+dx,y+dy); } public void rLineTo(float dx,float dy) { lineTo(x+dx,y+dy); }
 public void reset() { s=new StringBuilder(); x=y=startX=startY=0; fill=FillType.WINDING; }
 public void setFillType(FillType f) { fill=f; } public FillType getFillType() { return fill; }
 public String indroidPath() { return s.toString(); }
 public void addRect(float l,float t,float r,float b,Direction d) { moveTo(l,t); if(d==Direction.CW) { lineTo(r,t); lineTo(r,b); lineTo(l,b); } else { lineTo(l,b); lineTo(r,b); lineTo(r,t); } close(); }
 public void addRect(RectF r,Direction d) { addRect(r.left,r.top,r.right,r.bottom,d); }
 public void addCircle(float x,float y,float r,Direction d) { addOval(x-r,y-r,x+r,y+r,d); }
 public void addOval(RectF r,Direction d) { addOval(r.left,r.top,r.right,r.bottom,d); }
 public void addOval(float l,float t,float r,float b,Direction d) { float x=(l+r)/2,y=(t+b)/2,rx=(r-l)/2,ry=(b-t)/2,k=.55228475f; moveTo(x+rx,y); float sign=d==Direction.CW?1:-1; cubicTo(x+rx,y+sign*k*ry,x+k*rx,y+sign*ry,x,y+sign*ry); cubicTo(x-k*rx,y+sign*ry,x-rx,y+sign*k*ry,x-rx,y); cubicTo(x-rx,y-sign*k*ry,x-k*rx,y-sign*ry,x,y-sign*ry); cubicTo(x+k*rx,y-sign*ry,x+rx,y-sign*k*ry,x+rx,y); close(); }
 public void addRoundRect(RectF r,float rx,float ry,Direction d) { addRoundRect(r.left,r.top,r.right,r.bottom,rx,ry,d); }
 public void addRoundRect(float l,float t,float r,float b,float rx,float ry,Direction d) { rx=Math.min(Math.abs(rx),(r-l)/2); ry=Math.min(Math.abs(ry),(b-t)/2); if(d==Direction.CCW) { moveTo(l+rx,t); quadTo(l,t,l,t+ry); lineTo(l,b-ry); quadTo(l,b,l+rx,b); lineTo(r-rx,b); quadTo(r,b,r,b-ry); lineTo(r,t+ry); quadTo(r,t,r-rx,t); lineTo(l+rx,t); close(); return; } moveTo(l+rx,t); lineTo(r-rx,t); quadTo(r,t,r,t+ry); lineTo(r,b-ry); quadTo(r,b,r-rx,b); lineTo(l+rx,b); quadTo(l,b,l,b-ry); lineTo(l,t+ry); quadTo(l,t,l+rx,t); close(); }
 public void rewind(){reset();} public boolean isEmpty(){return s.length()==0;} public boolean isConvex(){return true;} public void incReserve(int extra){} public void toggleInverseFillType(){fill=FillType.values()[fill.ordinal()^2];} public boolean isInverseFillType(){return fill.ordinal()>=2;} public void rQuadTo(float a,float b,float c,float d){quadTo(x+a,y+b,x+c,y+d);} public void rCubicTo(float a,float b,float c,float d,float e,float f){cubicTo(x+a,y+b,x+c,y+d,x+e,y+f);} public void setLastPoint(float a,float b){lineTo(a,b);} public void addPath(Path p){addPath(p,new Matrix());} public void addPath(Path p,float x,float y){Matrix m=new Matrix();m.setTranslate(x,y);addPath(p,m);} public void addPath(Path p,Matrix m){Path copy=new Path(p);copy.transform(m);if(s.length()>0&&copy.s.length()>0)s.append(' ');s.append(copy.s.toString());x=copy.x;y=copy.y;}
 public void arcTo(RectF r,float start,float sweep){arcTo(r,start,sweep,false);} public void arcTo(RectF r,float start,float sweep,boolean force){int n=Math.max(1,(int)Math.ceil(Math.abs(sweep)/10));for(int i=0;i<=n;i++){double a=(start+sweep*i/n)*Math.PI/180;float xx=r.centerX()+r.width()*.5f*(float)Math.cos(a),yy=r.centerY()+r.height()*.5f*(float)Math.sin(a);if(i==0&&force)moveTo(xx,yy);else lineTo(xx,yy);}} public void addArc(RectF r,float start,float sweep){arcTo(r,start,sweep,true);} public void addArc(float l,float t,float r,float b,float start,float sweep){addArc(new RectF(l,t,r,b),start,sweep);}
 public void transform(Matrix m){transform(m,this);} public void transform(Matrix m,Path dst){String[] parts=s.toString().split(" ");Path out=new Path();out.fill=fill;int i=0;while(i<parts.length){String c=parts[i++];if(c.length()==0)continue;int n=c.equals("C")?6:c.equals("Q")?4:c.equals("Z")?0:2;float[] a=new float[n];for(int j=0;j<n;j++)a[j]=Float.parseFloat(parts[i++]);m.mapPoints(a);if(c.equals("M"))out.moveTo(a[0],a[1]);else if(c.equals("L"))out.lineTo(a[0],a[1]);else if(c.equals("Q"))out.quadTo(a[0],a[1],a[2],a[3]);else if(c.equals("C"))out.cubicTo(a[0],a[1],a[2],a[3],a[4],a[5]);else if(c.equals("Z"))out.close();}dst.set(out);} public void computeBounds(RectF out,boolean exact){String[] parts=s.toString().split(" ");float l=0,t=0,r=0,b=0;boolean first=true;int i=0;while(i<parts.length){String c=parts[i++];if(c.length()==0)continue;int n=c.equals("C")?6:c.equals("Q")?4:c.equals("Z")?0:2;for(int j=0;j<n;j+=2){float xx=Float.parseFloat(parts[i++]),yy=Float.parseFloat(parts[i++]);if(first){l=r=xx;t=b=yy;first=false;}else{l=Math.min(l,xx);r=Math.max(r,xx);t=Math.min(t,yy);b=Math.max(b,yy);}}}out.set(l,t,r,b);} public boolean op(Path p,Op op){return op(this,p,op);} public boolean op(Path a,Path b,Op op){RectF x=new RectF(),y=new RectF();a.computeBounds(x,true);b.computeBounds(y,true);Path result=new Path();if(op==Op.INTERSECT){if(x.intersect(y))result.addRect(x,Direction.CW);}else if(op==Op.UNION||op==Op.XOR){result.addPath(a);result.addPath(b);}else result.set(op==Op.DIFFERENCE?a:b);set(result);return true;} public float[] approximate(float error){RectF r=new RectF();computeBounds(r,true);return new float[]{0,r.left,r.top,1,r.right,r.bottom};}
}
