from pydantic_settings import BaseSettings


class ApiSettings(BaseSettings):
    debug: bool = False


class WorkerSettings(BaseSettings):
    debug: bool = False


class BillingSettings(BaseSettings):
    debug: bool = False
