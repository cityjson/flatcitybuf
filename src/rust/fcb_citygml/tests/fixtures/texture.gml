<?xml version="1.0" encoding="UTF-8"?>
<!-- One building with two polygons and one texture over it.

     The roof polygon is textured: the ParameterizedTexture targets it by the
     polygon's gml:id and states the texture coordinates against the ring's
     gml:id, which is why rings carry an id at all. The wall polygon is
     targeted by nothing, so its ring must come out as [null] under the same
     theme — CityJSON's "this ring has no texture".

     The ring is written closed, as GML rings are: five points, and five UV
     pairs to match. The reader drops the closing point, so the converted ring
     has four vertices and the trailing UV pair — the duplicate of the first —
     goes with it. Four texture vertices reach `vertices-texture`.

     The GeoreferencedTexture is valid CityGML with no CityJSON counterpart
     here: it must land in the report and not in the output, so nothing it
     names is textured. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                xmlns:app="http://www.opengis.net/citygml/appearance/2.0"
                xmlns:gml="http://www.opengis.net/gml"
                xmlns:xlink="http://www.w3.org/1999/xlink">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>1000 2000 0</gml:lowerCorner>
      <gml:upperCorner>1010 2006 3</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>

  <app:appearanceMember>
    <app:Appearance>
      <app:theme>rgbTexture</app:theme>
      <app:surfaceDataMember>
        <app:ParameterizedTexture gml:id="tex-roof">
          <app:imageURI>textures/roof.jpg</app:imageURI>
          <app:mimeType>image/jpeg</app:mimeType>
          <app:wrapMode>wrap</app:wrapMode>
          <app:target uri="#roof-1">
            <app:TexCoordList>
              <app:textureCoordinates ring="#roof-1-ring">0 0 1 0 1 1 0 1 0 0</app:textureCoordinates>
            </app:TexCoordList>
          </app:target>
        </app:ParameterizedTexture>
      </app:surfaceDataMember>
      <app:surfaceDataMember>
        <app:GeoreferencedTexture gml:id="tex-ortho">
          <app:imageURI>textures/ortho.tif</app:imageURI>
          <app:mimeType>image/tiff</app:mimeType>
          <app:target>#wall-1</app:target>
        </app:GeoreferencedTexture>
      </app:surfaceDataMember>
    </app:Appearance>
  </app:appearanceMember>

  <core:cityObjectMember>
    <bldg:Building gml:id="b1">
      <bldg:lod2MultiSurface>
        <gml:MultiSurface>
          <gml:surfaceMember>
            <gml:Polygon gml:id="roof-1">
              <gml:exterior>
                <gml:LinearRing gml:id="roof-1-ring">
                  <gml:posList>1000 2000 3 1010 2000 3 1010 2006 3 1000 2006 3 1000 2000 3</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
          <gml:surfaceMember>
            <gml:Polygon gml:id="wall-1">
              <gml:exterior>
                <gml:LinearRing gml:id="wall-1-ring">
                  <gml:posList>1000 2000 0 1010 2000 0 1010 2000 3 1000 2000 3 1000 2000 0</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
        </gml:MultiSurface>
      </bldg:lod2MultiSurface>
    </bldg:Building>
  </core:cityObjectMember>
</core:CityModel>
