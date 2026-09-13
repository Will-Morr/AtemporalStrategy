/** Small, code-native silhouettes shared by selection and production controls. */
export function unitIcon(type: string, color: string): SVGSVGElement {
  const ns='http://www.w3.org/2000/svg', svg=document.createElementNS(ns,'svg');
  svg.setAttribute('viewBox','0 0 32 32');svg.setAttribute('class','unit-icon');svg.setAttribute('role','img');svg.setAttribute('aria-label',type);
  const shape=(tag:string,attrs:Record<string,string>)=>{const e=document.createElementNS(ns,tag);for(const [k,v] of Object.entries(attrs))e.setAttribute(k,v);svg.append(e);};
  svg.style.color=color;
  const base={fill:'currentColor',stroke:'#071019','stroke-width':'2'};
  if(['factory','turret','wall'].includes(type)) {
    shape('rect',{...base,x:'3',y:'3',width:'26',height:'26',rx:'2'});
    if(type==='factory'){for(const x of ['8','20'])shape('rect',{x,y:'8',width:'4',height:'9',fill:'#e5f5ff'});shape('rect',{x:'11',y:'22',width:'10',height:'7',fill:'#071019'});}
    else if(type==='turret'){shape('circle',{...base,cx:'16',cy:'16',r:'8',fill:'#e5f5ff'});shape('rect',{...base,x:'14',y:'1',width:'4',height:'15'});}
    else shape('path',{d:'M4 16H28',stroke:'#e5f5ff','stroke-width':'3'});
  } else if(['tank','artillery'].includes(type)) {
    for(const x of ['3','23'])shape('rect',{...base,x,y:'7',width:'6',height:'22'});
    shape('rect',{...base,x:'8',y:'7',width:'16',height:'19'});shape('circle',{...base,cx:'16',cy:'16',r:'5',fill:'#e5f5ff'});shape('rect',{...base,x:'14',y:'1',width:'4',height:'15'});
  } else if(type==='miner') {
    for(const x of ['3','23'])shape('rect',{...base,x,y:'8',width:'6',height:'21'});
    shape('rect',{...base,x:'7',y:'8',width:'18',height:'20'});shape('rect',{...base,x:'3',y:'3',width:'26',height:'6'});shape('rect',{x:'11',y:'12',width:'10',height:'3',fill:'#e1d9b5'});
  } else if(type==='constructor') {
    shape('rect',{...base,x:'7',y:'9',width:'18',height:'19'});shape('path',{d:'M5 14V3M27 14V3M10 18H22M16 12V24',stroke:'#ffe28a','stroke-width':'4'});
  } else {
    shape('path',{...base,d:type==='scout'?'M16 2L28 28H4Z':'M16 5L28 22H4Z'});
    if(type==='grinder')for(const cx of ['9','23'])shape('circle',{...base,cx,cy:'8',r:'6',fill:'#ffb69b'});
    else shape('circle',{cx:'16',cy:'15',r:'4',fill:'#e5f5ff'});
  }
  return svg;
}
