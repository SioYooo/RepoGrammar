from pydantic import BaseModel, field_validator


class UserCreate(BaseModel):
    email: str
    display_name: str

    @field_validator("email")
    @classmethod
    def normalize_email(cls, value: str) -> str:
        return value.lower()


class UserRead(BaseModel):
    id: int
    email: str
    display_name: str


class AccountRead(BaseModel):
    id: int
    owner_id: int
    name: str
