"""Synthetic TIFFs and pixel oracles using the Python production Pillow baseline.

Dev dependencies: Pillow==11.3.0 numpy==2.2.6 tifffile==2025.5.10.
No account, user raster, or application startup is involved.
"""
from __future__ import annotations
import json
import struct
import zlib
from pathlib import Path
import numpy as np
import tifffile
from PIL import Image

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/"rust/tests/fixtures/rasters"
GEO=[(33550,"d",3,(0.25,0.5,0.0),False),(33922,"d",6,(0,0,0,168000,401000,0),False)]


def packed(path,values,bits,palette=False,big_endian=False):
    """Small independent uncompressed TIFF writer for packed 1/2/4-bit samples."""
    h,w=values.shape
    endian=">" if big_endian else "<"
    data=bytearray()
    for row in values:
        encoded=bytearray((w*bits+7)//8)
        for i,v in enumerate(row):
            encoded[i*bits//8]|=int(v)<<(8-bits-(i*bits%8))
        data.extend(encoded)
    tags={256:(4,1,w),257:(4,1,h),258:(3,1,bits),259:(3,1,1),262:(3,1,3 if palette else 1),
          273:(4,1,0),277:(3,1,1),278:(4,1,h),279:(4,1,len(data))}
    if palette:
        n=1<<bits
        colors=np.array([[i*65535//(n-1) for i in range(n)],
                         [(n-1-i)*65535//(n-1) for i in range(n)],
                         [32768]*n],dtype=endian+"u2").tobytes()
        tags[320]=(3,3*n,colors)
    extra=bytearray();base=8+2+12*len(tags)+4
    entries=bytearray()
    for tag,(kind,count,value) in sorted(tags.items()):
        if tag==273:value=base+len(extra)+(len(colors) if palette and 320>tag else 0)
        if isinstance(value,bytes):
            field=struct.pack(endian+"I",base+len(extra));extra.extend(value)
        elif kind==3:field=struct.pack(endian+"H",value)+b"\0\0"
        else:field=struct.pack(endian+"I",value)
        entries.extend(struct.pack(endian+"HHI",tag,kind,count)+field)
    # The palette is after the directory and before pixel bytes.
    if palette:
        index=sorted(tags).index(273)*12+8
        entries[index:index+4]=struct.pack(endian+"I",base+len(extra))
    path.write_bytes((b"MM" if big_endian else b"II")+struct.pack(endian+"HI",42,8)
        +struct.pack(endian+"H",len(tags))+entries+struct.pack(endian+"I",0)+extra+data)


def patch_uint_tag(path,tag,value):
    data=bytearray(path.read_bytes())
    endian="<" if data[:2]==b"II" else ">"
    ifd=struct.unpack_from(endian+"I",data,4)[0]
    count=struct.unpack_from(endian+"H",data,ifd)[0]
    for index in range(count):
        offset=ifd+2+12*index
        if struct.unpack_from(endian+"H",data,offset)[0]==tag:
            struct.pack_into(endian+"HII",data,offset+2,4,1,value)
            path.write_bytes(data)
            return
    raise AssertionError(f"tag {tag} not found")


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    cases=[]
    def oracle(name,options=None,transparent=None,max_peak=None,expected_chunks=None,numeric_rgba=None,tolerance=0,native_difference=None):
        if numeric_rgba is None:
            with Image.open(OUT/(name+".tiff")) as image:
                rgba=image.convert("RGBA")
        else:rgba=Image.fromarray(numeric_rgba).convert("RGBA")
        if transparent:
            for x,y in transparent:rgba.putpixel((x,y),(0,0,0,0))
        side=(options or {}).get("max_side",2048)
        pixels=(options or {}).get("max_pixels",2048*2048)
        scale=min(1,side/rgba.width,side/rgba.height,(pixels/(rgba.width*rgba.height))**0.5)
        w=max(1,int(rgba.width*scale));h=max(1,int(rgba.height*scale))
        while w*h>pixels:
            if w>=h:w-=1
            else:h-=1
        if (w,h)!=rgba.size:
            resized=rgba.resize((w,h),Image.Resampling.NEAREST);rgba.close();rgba=resized
        case=dict(tolerance=tolerance,file=name+".tiff",width=w,height=h,crc32=zlib.crc32(rgba.tobytes()),options=options,oracle="numeric-sample-values" if numeric_rgba is not None else "Pillow-11.3.0")
        if w*h<=2048:case["rgba"]=list(rgba.tobytes())
        if max_peak is not None:case["max_peak_chunk_bytes"]=max_peak
        if expected_chunks is not None:case["expected_chunks"]=expected_chunks
        if transparent:case["native_difference"]="Nonfinite/GDAL NoData samples are transparent instead of opaque black."
        if native_difference:case["native_difference"]=native_difference
        rgba.close();cases.append(case)
    def write(name,data,photo="rgb",**kwargs):
        extra=kwargs.pop("extratags",[])
        numeric=kwargs.pop("numeric_rgba",None)
        difference=kwargs.pop("native_difference",None)
        tifffile.imwrite(OUT/(name+".tiff"),data,photometric=photo,metadata=None,extratags=GEO+extra,**kwargs)
        oracle(name,numeric_rgba=numeric,native_difference=difference)
    rgb=(np.arange(19*17*3,dtype=np.uint16).reshape(19,17,3)*19%256).astype(np.uint8)
    write("rgb8-strips",rgb,rowsperstrip=3)
    write("rgb8-tile-edges",rgb,tile=(16,16))
    tifffile.imwrite(OUT/"uintmax-rows-per-strip.tiff",rgb,photometric="rgb",metadata=None,rowsperstrip=19,extratags=GEO)
    patch_uint_tag(OUT/"uintmax-rows-per-strip.tiff",278,0xFFFFFFFF)
    oracle("uintmax-rows-per-strip")
    write("rgb8-planar-strips",rgb.transpose(2,0,1),planarconfig="separate",rowsperstrip=3)
    write("rgb8-planar-tile-edges",rgb.transpose(2,0,1),planarconfig="separate",tile=(16,16))
    write("rgb16-big-endian",rgb.astype(np.uint16)*257+np.uint16(128),byteorder=">")
    write("rgb16-planar-tile-edges",(rgb.astype(np.uint16)*257+np.uint16(128)).transpose(2,0,1),planarconfig="separate",tile=(16,16),byteorder=">",
        numeric_rgba=((rgb.astype(np.uint16)*257+np.uint16(128))>>8).astype(np.uint8),
        native_difference="Pillow 11.3.0 planar RGB16 treats successive bytes as pixels; native uses each numeric 16-bit sample, taking its high byte.")
    rgba=np.concatenate([rgb,np.full((19,17,1),128,dtype=np.uint8)],axis=2)
    write("rgba8-unassociated",rgba,extrasamples=["UNASSALPHA"],rowsperstrip=4)
    associated=rgba.copy();associated[:,:,:3]//=2
    write("rgba8-associated",associated,extrasamples=["ASSOCALPHA"],rowsperstrip=4)
    write("rgba16-associated",associated.astype(np.uint16)*257,extrasamples=["ASSOCALPHA"])
    write("rgba16-alpha-edge-values",np.array([[[128,64,1,257],[1,1,1,1],[129,1,128,65534],[65534,32767,129,65535]]],np.uint16),extrasamples=["ASSOCALPHA"])
    write("rgba8-planar-tile-edges",rgba.transpose(2,0,1),planarconfig="separate",tile=(16,16),extrasamples=["UNASSALPHA"])
    gray=np.array([[0,128,129,255,256,32767,65535]],np.uint16)
    write("gray16",gray,"minisblack")
    write("gray16-white-big-endian",gray,"miniswhite",byteorder=">",numeric_rgba=np.clip(65535-gray,0,255).astype(np.uint8))
    write("gray16-bigtiff-white",gray,"miniswhite",bigtiff=True,numeric_rgba=np.clip(65535-gray,0,255).astype(np.uint8))
    write("gray8-white",np.arange(12,dtype=np.uint8).reshape(3,4)*20,"miniswhite")
    write("gray-signed",np.array([[-128,-1,0,128,255,256,32767]],np.int16),"minisblack")
    write("gray-float",np.array([[-1,0,12.9,128.9,255.9,300]],np.float32),"minisblack")
    gray_alpha=np.array([[[10,128],[200,255],[0,0]]],np.uint8)
    write("gray-alpha",gray_alpha,"minisblack",extrasamples=["UNASSALPHA"])
    write("gray-alpha-associated",gray_alpha,"minisblack",extrasamples=["ASSOCALPHA"],numeric_rgba=np.array([[[19,19,19,128],[200,200,200,255],[0,0,0,0]]],np.uint8))
    write("cmyk8",np.array([[[10,20,30,40],[255,0,0,0],[0,0,0,255]]],np.uint8),"separated")
    write("cmyk8-planar",np.tile(np.array([[[10]],[[20]],[[30]],[[40]]],np.uint8),(1,3,5)),"separated",planarconfig="separate",rowsperstrip=1)
    write("ycbcr8",np.array([[[0,128,128],[255,128,128],[100,50,200],[90,220,10]]],np.uint8),"ycbcr",subsampling=(1,1),compression="deflate")
    palette=np.array([np.arange(65536,dtype=np.uint16),65535-np.arange(65536,dtype=np.uint16),np.full(65536,32768,np.uint16)])
    indices=np.array([[0,1,257,50000,65535]],np.uint16)
    expected=np.stack([palette[c,indices]>>8 for c in range(3)],axis=-1).astype(np.uint8)
    write("palette16-bigtiff",indices,"palette",colormap=palette,bigtiff=True,numeric_rgba=expected)
    for bits in [1,2,4,8]:
        values=(np.arange(3*11).reshape(3,11)%(1<<bits)).astype(np.uint8)
        packed(OUT/f"palette{bits}.tiff",values,bits,palette=True,big_endian=bits==4);oracle(f"palette{bits}")
        packed(OUT/f"gray{bits}-packed.tiff",values,bits);oracle(f"gray{bits}-packed")
    small=np.array([[[1,0,0],[2,0,0],[3,0,0]],[[4,0,0],[5,0,0],[6,0,0]]],np.uint8)
    for o in range(1,9):write(f"orientation{o}",small,extratags=[(274,"H",1,o,False)])
    for compression in ["tiff_lzw","tiff_adobe_deflate","packbits"]:
        path=OUT/(compression+".tiff")
        Image.fromarray(rgb).save(path,compression=compression);oracle(compression)
    # Independent JPEG IDCT implementations can differ by up to two 8-bit levels.
    Image.fromarray(rgb).save(OUT/"jpeg.tiff",compression="jpeg")
    oracle("jpeg",tolerance=2)
    # NoData/NaN transparency is an intentional native improvement over Pillow.
    missing=np.array([[10,np.nan,-9999,20]],np.float32)
    tifffile.imwrite(OUT/"nodata-float.tiff",missing,photometric="minisblack",metadata=None,extratags=[(42113,"s",0,"-9999",False)])
    oracle("nodata-float",transparent=[(1,0),(2,0)])
    # A compressed >16M pixel source: the old native reader refused this outright.
    large=np.zeros((4097,4097,3),np.uint8)
    large[:,:,0]=np.arange(4097,dtype=np.uint32)%256
    large[:,:,1]=(np.arange(4097,dtype=np.uint32)%256)[:,None]
    large[:,:,2]=17
    tifffile.imwrite(OUT/"large-striped.tiff",large,photometric="rgb",metadata=None,compression="deflate",rowsperstrip=16,extratags=GEO)
    oracle("large-striped",max_peak=4097*16*3,expected_chunks=256)
    thin=np.arange(4097,dtype=np.uint16).reshape(4097,1)%256
    tifffile.imwrite(OUT/"thin-skipped-strips.tiff",thin.astype(np.uint8),photometric="minisblack",metadata=None,rowsperstrip=1)
    oracle("thin-skipped-strips",options=dict(max_side=8,max_pixels=64),max_peak=1,expected_chunks=8)
    (OUT/"manifest.json").write_text(json.dumps(cases,indent=2)+"\n",encoding="utf-8")
    print(f"Generated {len(cases)} TIFF pixel fixtures under {OUT}")


if __name__=="__main__":main()
