! Bounded parse-degraded fixture: preprocessor directives. The preprocessed
! text this file would become is not decidable here, so the file abstains.
#ifdef WITH_TESTDRIVE
module preprocessed
  use testdrive
contains
  subroutine test_guarded(error)
    type(error_type), allocatable, intent(out) :: error
  end subroutine test_guarded
end module preprocessed
#endif
