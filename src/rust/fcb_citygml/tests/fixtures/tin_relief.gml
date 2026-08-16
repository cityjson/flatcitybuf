<?xml version="1.0" encoding="UTF-8"?>
<!-- A terrain, written the way CityGML usually writes one: a dem:ReliefFeature
     wrapping its components rather than a bare dem:TINRelief.

     The wrapper is not a city object CityJSON has a type for, so it is skipped
     with a note and each dem:reliefComponent that holds a TINRelief becomes a
     top-level object of its own. One member therefore yields one feature here,
     and would yield two if the feature held two components.

     The TIN itself is the smallest one that is still a surface: two triangles
     sharing an edge, written as gml:Triangle patches of a
     gml:TriangulatedSurface. They come out as a CityJSON CompositeSurface —
     the patches of a TIN are a connected surface, not a loose collection — at
     the LoD the dem:lod element states.

     The corner at (100, 200, 0) is the document's lower corner and so its
     translate; the third and fourth points climb to z = 1 and z = 2, so the
     extent is a box rather than a plane. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:dem="http://www.opengis.net/citygml/relief/2.0"
                xmlns:gml="http://www.opengis.net/gml">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>100 200 0</gml:lowerCorner>
      <gml:upperCorner>102 201 2</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>
  <core:cityObjectMember>
    <dem:ReliefFeature gml:id="relief-1">
      <gml:name>Terrain</gml:name>
      <dem:lod>1</dem:lod>
      <dem:reliefComponent>
        <dem:TINRelief gml:id="tin-1">
          <gml:name>Terrain patch</gml:name>
          <dem:lod>1</dem:lod>
          <dem:tin>
            <gml:TriangulatedSurface>
              <gml:trianglePatches>
                <gml:Triangle>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>100 200 0 102 200 0 102 201 1 100 200 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Triangle>
                <gml:Triangle>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>100 200 0 102 201 1 100 201 2 100 200 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Triangle>
              </gml:trianglePatches>
            </gml:TriangulatedSurface>
          </dem:tin>
        </dem:TINRelief>
      </dem:reliefComponent>
    </dem:ReliefFeature>
  </core:cityObjectMember>
</core:CityModel>
