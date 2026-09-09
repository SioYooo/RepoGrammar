package com.example.web;

import jakarta.servlet.annotation.WebServlet;
import jakarta.servlet.http.HttpServlet;

// Three exact Jakarta Servlet anchors, enough to reach Java's minimum family
// support of three.
@WebServlet("/catalog")
public class CatalogServlet extends HttpServlet {
}

@WebServlet("/orders")
class OrderServlet extends HttpServlet {
}

@WebServlet("/shipments")
class ShipmentServlet extends HttpServlet {
}
