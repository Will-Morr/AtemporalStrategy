export interface PlotPoint { x: number; y: number | null }

/** Local linear slope over at most five adjacent samples; never smooth across missing values. */
export function slopePoints(points: PlotPoint[]): PlotPoint[] {
  return points.map((point,index)=>{
    if(point.y===null)return {...point};
    let lo=index,hi=index;
    while(lo>Math.max(0,index-2)&&points[lo-1].y!==null)lo--;
    while(hi<Math.min(points.length-1,index+2)&&points[hi+1].y!==null)hi++;
    const window=points.slice(lo,hi+1);
    const meanX=window.reduce((n,p)=>n+p.x,0)/window.length;
    const meanY=window.reduce((n,p)=>n+p.y!,0)/window.length;
    const variance=window.reduce((n,p)=>n+(p.x-meanX)**2,0);
    return {x:point.x,y:variance>0?window.reduce((n,p)=>n+(p.x-meanX)*(p.y!-meanY),0)/variance:null};
  });
}
