"""Compare native system-font discovery to the original fresh local catalog."""
from pathlib import Path
import json,os,subprocess,sys
from ezdxf.fonts.font_manager import FontManager
from ezdxf.fonts import fonts
from unittest.mock import patch
from generate_font_resolution_fixtures import actual_path
def main():
    manager=FontManager();manager.build()
    queries=['ArialUni.ttf','arial.ttf','ARIAL.TTF','NLCS-ISO.ttf','LiberationMono-Regular.ttf','txt','monospace','serif','sans-serif','MONOSPACE','Sans-Serif','romans.shx','missing-sleufbase-font.ttf','unknown.ext','*monospace']
    result=subprocess.run([str(Path(sys.argv[1]).resolve()),*queries],capture_output=True,text=True,encoding='utf-8',timeout=120)
    assert result.returncode==0,result.stderr
    actual=json.loads(result.stdout)
    for query in queries:
        fonts.TrueTypeFont._glyph_caches.clear()
        with patch.object(fonts,'font_manager',manager):selected=actual_path(fonts.make_font(query,1.),manager)
        if selected is None:assert actual[query] is None,(query,actual[query])
        else:assert actual[query] is not None and os.path.samefile(selected,actual[query]),(query,str(selected),actual[query])
    print(f'{len(queries)} original/native system font selections match, including family lookup, case and missing-font fallbacks')
if __name__=='__main__':main()
