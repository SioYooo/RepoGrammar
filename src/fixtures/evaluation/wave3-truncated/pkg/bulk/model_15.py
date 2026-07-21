from pydantic import BaseModel


class Item15(BaseModel):
    id: int
    label: str
    quantity: int
