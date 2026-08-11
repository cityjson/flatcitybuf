<?xml version="1.0" encoding="UTF-8"?>
<!-- One building carrying every attribute shape this converter maps: the GML
     name, four thematic building properties (two of them numeric, two
     gml:CodeType), and one generic attribute of each of the six typed kinds.
     The geometry is a single 10 m square at the origin, so the transform is a
     zero translate and the quantised vertices read by eye. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                xmlns:gen="http://www.opengis.net/citygml/generics/2.0"
                xmlns:gml="http://www.opengis.net/gml">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>0 0 0</gml:lowerCorner>
      <gml:upperCorner>10 10 0</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>
  <core:cityObjectMember>
    <bldg:Building gml:id="b1">
      <gml:name>Town Hall</gml:name>
      <bldg:function>1000</bldg:function>
      <bldg:roofType>1030</bldg:roofType>
      <bldg:yearOfConstruction>1985</bldg:yearOfConstruction>
      <!-- The uom is not part of the CityJSON value: it is dropped. -->
      <bldg:measuredHeight uom="m">9.5</bldg:measuredHeight>
      <gen:stringAttribute name="owner">
        <gen:value>Acme Property BV</gen:value>
      </gen:stringAttribute>
      <gen:intAttribute name="floorCount">
        <gen:value>7</gen:value>
      </gen:intAttribute>
      <gen:doubleAttribute name="floorArea">
        <gen:value>842.5</gen:value>
      </gen:doubleAttribute>
      <gen:dateAttribute name="surveyDate">
        <gen:value>2019-03-04</gen:value>
      </gen:dateAttribute>
      <gen:uriAttribute name="reference">
        <gen:value>https://example.org/buildings/b1</gen:value>
      </gen:uriAttribute>
      <gen:measureAttribute name="volume">
        <gen:value uom="m3">2530.75</gen:value>
      </gen:measureAttribute>
      <bldg:lod0MultiSurface>
        <gml:MultiSurface>
          <gml:surfaceMember>
            <gml:Polygon>
              <gml:exterior>
                <gml:LinearRing>
                  <gml:posList>0 0 0 10 0 0 10 10 0 0 10 0 0 0 0</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
        </gml:MultiSurface>
      </bldg:lod0MultiSurface>
    </bldg:Building>
  </core:cityObjectMember>
</core:CityModel>
