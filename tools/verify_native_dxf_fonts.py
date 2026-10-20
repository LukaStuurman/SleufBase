"""Compare native system-font discovery to the original fresh local catalog."""
from pathlib import Path
import json,os,subprocess,sys
from ezdxf.fonts.font_manager import FontManager
def main():
    manager=FontManager();manager.build()
    queries=['ArialUni.ttf','arial.ttf','ARIAL.TTF','NLCS-ISO.ttf','LiberationMono-Regular.ttf','txt','monospace','serif','sans-serif','MONOSPACE','Sans-Serif','romans.shx','missing-sleufbase-font.ttf','unknown.ext','*monospace']
    result=subprocess.run([str(Path(sys.argv[1]).resolve()),*queries],capture_output=True,text=True,encoding='utf-8',timeout=120)
    assert result.returncode==0,result.stderr
    actual=json.loads(result.stdout)
    for query in queries:
        if query=='*monospace':assert actual[query] is None;continue
        ext=Path(query).suffix.lower();selected=None
        if not ext:
            face=manager.find_best_match(family=query,style='.shx',italic=None)
            if face:selected=manager._font_cache.get(face.filename,manager.fallback_font_name()).file_path
        if selected is None:
            selected=manager._font_cache.get(query if ext in ('.ttf','.ttc','.otf','.shx','.shp','.lff') else manager.fallback_font_name(),manager.fallback_font_name()).file_path
        assert actual[query] is not None and os.path.samefile(selected,actual[query]),(query,str(selected),actual[query])
    print(f'{len(queries)} original/native system font selections match, including family lookup, case and missing-font fallbacks')
if __name__=='__main__':main()
