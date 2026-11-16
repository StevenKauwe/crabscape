/** Phrase replacements are separate from application text. Never append previews. */
export class Transcript {
  constructor() { this.finals = new Map(); this.live = new Map(); }
  update({type,phrase,text}) {
    if (!Number.isSafeInteger(phrase) || phrase < 0) throw new Error('Invalid phrase index');
    if (type === 'final') { this.finals.set(phrase,text); this.live.delete(phrase); }
    else if (type === 'live' && !this.finals.has(phrase)) this.live.set(phrase,text);
    let next = 0; const solid = [],soft = [];
    while(this.finals.has(next)) { solid.push(this.finals.get(next));next++; }
    const pending = new Set([...this.finals.keys(),...this.live.keys()]);
    for(const id of [...pending].sort((a,b)=>a-b)) if(id>=next) soft.push(this.finals.get(id) ?? this.live.get(id));
    return {committed:solid.filter(Boolean).join(' '),live:soft.filter(Boolean).join(' ')};
  }
  finish() {
    if(this.live.size) throw new Error('Recording ended with unfinished preview text.');
    const ids=[...this.finals.keys()].sort((a,b)=>a-b);
    if(ids.some((id,i)=>id!==i)) throw new Error('Recording ended with a missing final phrase.');
    return ids.map(id=>this.finals.get(id)).filter(Boolean).join(' ');
  }
}
