import silhouettes from '../unit-icons.json';

/** Shared vector assets used by game controls and the statically generated guide. */
export function unitIcon(type: string, color: string): SVGSVGElement {
  const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
  svg.setAttribute('viewBox','0 0 32 32');svg.setAttribute('class','unit-icon');
  svg.setAttribute('role','img');svg.setAttribute('aria-label',type);
  svg.style.color=color;
  // Trusted repository-authored SVG paths, never user-provided markup.
  svg.innerHTML=(silhouettes as Record<string,string>)[type]??silhouettes.default;
  return svg;
}
