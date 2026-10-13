// Generates parity fixtures from the embedded Spline Armory engine.
// node tools/gen_fixtures.js <three.min.js> <sa_engine.js> <outdir>
const fs=require('fs'), vm=require('vm'), path=require('path');
const [threePath, enginePath, out]=process.argv.slice(2);
const fakeCtx={createRadialGradient:()=>({addColorStop(){}}),fillRect(){},set fillStyle(v){}};
const ctx={console,Math,JSON,Date,Map,Set,Array,Object,Number,String,Uint8Array,Float32Array,Uint16Array,Uint32Array,Int32Array,
 document:{createElement:()=>({width:0,height:0,getContext:()=>fakeCtx})},self:{},window:{}};
ctx.globalThis=ctx; ctx.window=ctx; ctx.self=ctx;
vm.createContext(ctx);
vm.runInContext(fs.readFileSync(threePath,'utf8'),ctx);
vm.runInContext('var outlineMat={};\n'+fs.readFileSync(enginePath,'utf8')+'\nthis.SA=SA;',ctx);
const SA=ctx.SA;
// mulberry32 reference streams
const mul=a=>()=>{a|=0;a=a+0x6D2B79F5|0;let t=Math.imul(a^a>>>15,1|a);t=t+Math.imul(t^t>>>7,61|t)^t;return((t^t>>>14)>>>0)/4294967296;};
const rng={note:'mulberry32(seed) outputs in [0,1). The Armory randomizer seeds it with (seed*2654435761)>>>0 (u64 multiply, keep low 32 bits).',streams:{}};
for(const s of [0,1,2,7,1207,4127,999999,(1207*2654435761)>>>0]){const r=mul(s); rng.streams[s]=Array.from({length:24},r);}
fs.mkdirSync(path.join(out,'rng'),{recursive:true}); fs.writeFileSync(path.join(out,'rng','mulberry32.json'),JSON.stringify(rng,null,1));
// Armory states: SA.roll(family, seed-1) yields state.seed===seed (roll uses (seed+i*7919)%999983+1, first try i=0)
const fams=['staff','mace','sword','spear','dagger','bow','crossbow','helm','garment','hands','feet','shield','ring','amulet','charm','bags','chests','food'];
fs.mkdirSync(path.join(out,'armory'),{recursive:true});
const N=+process.env.N||200, index={};
for(const f of fams){const rows=[];
 for(let seed=1;seed<=N;seed++){const st=SA.roll(f,seed-1,null); if(st.seed!==seed) throw new Error('seed mapping broke for '+f); rows.push({seed,name:SA.noun(st),state:st});}
 fs.writeFileSync(path.join(out,'armory',f+'.json'),JSON.stringify({family:f,count:rows.length,rows}));
 index[f]=rows.length;}
// presets, base state, labels
fs.writeFileSync(path.join(out,'armory','_base.json'),JSON.stringify({BASE:SA.BASE,PRESETS:SA.PRESETS,FAM_LABELS:SA.FAM_LABELS,FAM_GROUPS:SA.FAM_GROUPS},null,1));
console.log('ok',index);
