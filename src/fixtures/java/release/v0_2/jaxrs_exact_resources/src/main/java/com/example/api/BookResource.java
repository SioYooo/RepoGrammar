package com.example.api;

import jakarta.ws.rs.GET;
import jakarta.ws.rs.Path;

@Path("/books")
public class BookResource {
    @GET
    @Path("/list")
    public String list() {
        return "list";
    }

    @GET
    @Path("/count")
    public String count() {
        return "count";
    }

    @GET
    @Path("/latest")
    public String latest() {
        return "latest";
    }
}
