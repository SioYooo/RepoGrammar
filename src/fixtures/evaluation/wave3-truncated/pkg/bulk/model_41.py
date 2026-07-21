from pydantic import BaseModel


class Item41(BaseModel):
    id: int
    label: str
    quantity: int
