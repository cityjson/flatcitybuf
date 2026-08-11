<?xml version="1.0" encoding="UTF-8"?>
<!-- A single LoD1 building: the unit cube whose lower corner is at
     (1000, 2000, 0), as a gml:Solid with one exterior CompositeSurface of six
     closed polygons. The offset makes a converter that forgets to translate
     obvious, and the face order below is the one tests/building_model.rs
     already pins, so the two agree on which vertex is seen first. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                xmlns:gml="http://www.opengis.net/gml"
                xmlns:xlink="http://www.w3.org/1999/xlink">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>1000 2000 0</gml:lowerCorner>
      <gml:upperCorner>1001 2001 1</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>
  <core:cityObjectMember>
    <bldg:Building gml:id="b1">
      <bldg:lod1Solid>
        <gml:Solid>
          <gml:exterior>
            <gml:CompositeSurface>
              <!-- bottom, z = 0 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2000 0 1001 2000 0 1001 2001 0 1000 2001 0 1000 2000 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <!-- top, z = 1 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2000 1 1000 2001 1 1001 2001 1 1001 2000 1 1000 2000 1</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <!-- y = 2000 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2000 0 1000 2000 1 1001 2000 1 1001 2000 0 1000 2000 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <!-- y = 2001 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1001 2001 0 1001 2001 1 1000 2001 1 1000 2001 0 1001 2001 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <!-- x = 1000 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2001 0 1000 2001 1 1000 2000 1 1000 2000 0 1000 2001 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <!-- x = 1001 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1001 2000 0 1001 2000 1 1001 2001 1 1001 2001 0 1001 2000 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:CompositeSurface>
          </gml:exterior>
        </gml:Solid>
      </bldg:lod1Solid>
    </bldg:Building>
  </core:cityObjectMember>
</core:CityModel>
