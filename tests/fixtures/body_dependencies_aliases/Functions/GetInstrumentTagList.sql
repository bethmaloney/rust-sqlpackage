-- Scalar function with nested subquery and table alias
-- Tests alias resolution in scalar function bodies
CREATE FUNCTION [dbo].[GetInstrumentTagList]
(
    @InstrumentId INT
)
RETURNS NVARCHAR(MAX)
AS
BEGIN
    DECLARE @TagList NVARCHAR(MAX);

    -- Use STUFF with nested subquery containing alias ITAG
    SELECT @TagList = STUFF((
        SELECT ', ' + [ITAG].[Name]
        FROM [dbo].[InstrumentTag] [ITA]
        INNER JOIN [dbo].[Tag] [ITAG] ON [ITA].TagId = [ITAG].Id
        WHERE [ITA].InstrumentId = @InstrumentId
        ORDER BY [ITAG].[Name]
        FOR XML PATH('')
    ), 1, 2, '');

    RETURN ISNULL(@TagList, '');
END;
GO
