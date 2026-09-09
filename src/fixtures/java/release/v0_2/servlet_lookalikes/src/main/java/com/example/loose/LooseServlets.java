package com.example.loose;

// A local @WebServlet annotation with no import and no fully qualified name is
// not the Jakarta or javax one, so nothing here may anchor.
@interface WebServlet {
    String value();
}

@WebServlet("/catalog")
class CatalogServlet {
}

@WebServlet("/orders")
class OrderServlet {
}

@WebServlet("/shipments")
class ShipmentServlet {
}
