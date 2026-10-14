// Pulls the game data tables out of the prototype page into JSON (functions kept as source strings).
// node tools/extract_data.js <plate.html> <outdir>
const fs=require('fs'), vm=require('vm'), path=require('path');
const [html,out]=process.argv.slice(2); const src=fs.readFileSync(html,'utf8');
function grab(name){const i=src.indexOf('const '+name+'='); if(i<0) throw new Error(name); let j=src.indexOf('=',i)+1, d=0, k=j, q=null;
 for(;k<src.length;k++){const c=src[k]; if(q){ if(c==='\\'){k++;continue;} if(c===q) q=null; continue;} if(c==='"'||c==="'"||c==='`'){q=c;continue;}
  if('{[('.includes(c)) d++; else if('}])'.includes(c)){d--; if(d===0){k++;break;}} }
 return src.slice(i,k)+';';}
// SAB and AFX are not extracted: SAB's filters and tweaks are typed Rust in
// crates/fb_items/src/armory_base.rs, and data/afx.json's effects are ported by hand to typed modifiers.
const names=['RACES','HAIR','HAIR_LIZ','HAIR_GOB','CLOTH','TRIM','LEATHER','EYES','SLOTS','BASES','GOODS','OUTFITS','SYL','PLACES','FACTIONS','FOOD_DESC','FAMILIES','SLOT_FAM','MAT_HEX'];
const pre=`const inS=(k,f,arr)=>s=>arr.includes(s[k][f]); const SB=(fam,acc,extra={})=>Object.assign({fam,acc},extra);`;
const code=pre+names.map(grab).join('\n')+`\nthis.D={${names.join(',')}};`;
const ctx={}; vm.createContext(ctx); vm.runInContext(code,ctx);
const ser=v=>JSON.stringify(v,(k,x)=>typeof x==='function'?'fn: '+x.toString():x,1);
fs.mkdirSync(out,{recursive:true});
for(const n of names) fs.writeFileSync(path.join(out,n.toLowerCase()+'.json'),ser(ctx.D[n]));
// constants
const consts={}; for(const c of ['BLADE_SCALE','BOW_SCALE','BOW_MAX','SHIELD_SCALE','SHIELD_MAX','GRIP_MAX']){const m=src.match(new RegExp('\\b'+c+'=([^,;]+)')); consts[c]=m&&m[1];}
const bt=src.match(/BLADE_TURN=([^;]+);/); consts.BLADE_TURN=bt&&bt[1];
fs.writeFileSync(path.join(out,'constants.json'),JSON.stringify(consts,null,1));
console.log('wrote',names.length+1,'tables',consts);
