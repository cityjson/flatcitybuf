<?xml version="1.0" encoding="UTF-8"?>
<!-- Geometry that is valid CityGML and has nowhere to go in CityJSON. The
     building keeps the one surface that can be written; the other two are
     reported in dropped_geometry.expected.report.txt and the document
     survives both.

     The 2D surface is the case that used to be silent: ten coordinates is
     not a multiple of three, so before srsDimension was read this document
     failed outright — and a 2D ring of six points, which does divide by
     three, was regrouped into four points nowhere near the building. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                xmlns:gml="http://www.opengis.net/gml">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>1000 2000 0</gml:lowerCorner>
      <gml:upperCorner>1001 2001 0</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>
  <core:cityObjectMember>
    <bldg:Building gml:id="b1">
      <bldg:lod0MultiSurface>
        <gml:MultiSurface>
          <!-- Written: the square at z = 0. -->
          <gml:surfaceMember>
            <gml:Polygon>
              <gml:exterior>
                <gml:LinearRing>
                  <gml:posList>1000 2000 0 1001 2000 0 1001 2001 0 1000 2001 0 1000 2000 0</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
          <!-- Dropped: the same square with no z at all. -->
          <gml:surfaceMember>
            <gml:Polygon>
              <gml:exterior>
                <gml:LinearRing>
                  <gml:posList srsDimension="2">1000 2000 1001 2000 1001 2001 1000 2001 1000 2000</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
        </gml:MultiSurface>
      </bldg:lod0MultiSurface>
      <!-- Dropped: a level-of-detail property CityJSON has no member for. -->
      <bldg:lod2TerrainIntersection>
        <gml:MultiCurve>
          <gml:curveMember>
            <gml:LineString>
              <gml:posList>1000 2000 0 1001 2000 0</gml:posList>
            </gml:LineString>
          </gml:curveMember>
        </gml:MultiCurve>
      </bldg:lod2TerrainIntersection>
    </bldg:Building>
  </core:cityObjectMember>
</core:CityModel>
